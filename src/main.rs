use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

use priv_rec_cert::prc::config::{DBConfig, SystemConfig, localhost_peer_config};
use priv_rec_cert::prc::util::get_cpu_time;
use priv_rec_cert::prc::{client::PRCClient, server::PRCServer};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

use clap::Parser;

/// CLI base on Clap
#[derive(Parser, Debug)]
#[command(version = "0.2")]
#[command(about = "Runs a multiparty credential non-revocation check protocol")]
struct Cli {
    /// Peer ID
    #[arg(long)]
    id: usize,

    /// Number of parties
    #[arg(long, default_value_t = 2)]
    party_num: usize,

    /// DB's 1st dimension size.
    /// Must be a power of 2 and larger or equal to db_l2.
    #[arg(long, default_value_t = 1024)]
    db_l1: usize,
    /// DB's 2nd dimension size.
    /// Must be a power of 2.
    #[arg(long, default_value_t = 1024)]
    db_l2: usize,

    /// Number of repetitions
    #[arg(long, default_value_t = 10)]
    rep_num: usize,

    /// Base port for the server. ports will be assigned sequentially starting from this port.
    #[arg(long, default_value_t = 8000)]
    base_port: usize,

    /// Append the measurement results to the output file
    #[arg(long)]
    out_fs: String,
}

#[tokio::main]
async fn main() {
    if !is_x86_feature_detected!("avx2") {
        panic!("AVX2 required for tests, but not available on this machine.");
    }
    env_logger::builder().try_init().ok();
    log::info!("passed avx2 check");

    let cli_args = Cli::parse();
    log::info!("Running with CLI args: {:?}", cli_args);
    run_server(cli_args).await;
    // run_channel_rec_retrieve(cli_args).await;
    log::info!("finished running.");
}

fn get_measurement_file(cli_args: &Cli) -> Option<std::fs::File> {
    if !cli_args.out_fs.is_empty() {
        log::info!(
            "Measurement results will be written to: {}",
            cli_args.out_fs
        );
        Some(
            OpenOptions::new()
                .create(true)
                .append(true) // uses O_APPEND flag
                .open(&cli_args.out_fs)
                .expect("Failed to open output file"),
        )
    } else {
        log::warn!("No output file specified for measurements. Results will not be saved.");
        None
    }
}

async fn run_server(cli_args: Cli) {
    let party_num = cli_args.party_num;
    let max_req_num = cli_args.rep_num;
    let server_id = cli_args.id;

    let peer_info = localhost_peer_config(party_num, cli_args.base_port as u16);
    let server_db_config = DBConfig::new(1, cli_args.db_l1, cli_args.db_l2, Some([42; 32]));
    let client_db_config = DBConfig::new(1, cli_args.db_l1, cli_args.db_l2, None);
    let config = SystemConfig {
        peers: peer_info.clone(),
        db_config: server_db_config,
    };

    let client = PRCClient::new(party_num, client_db_config, config.clone());
    let mut server = PRCServer::new(server_id, config.clone(), max_req_num).await;
   
    // set measurement file
    let mut measure_fs = get_measurement_file(&cli_args);

    // ask rep
    let mut rng = ChaCha8Rng::from_seed([13; 32]);
    for _ in 0..max_req_num {
        let wall_timer = Instant::now();
        let cpu_timer = get_cpu_time();

        // no check for correctness in performance test
        let idx = rng.next_u32() as usize % client.total_db_size();

        let (st, query) = client.query_single_server(idx, server_id, &mut rng);
        // let (l1_idx_share, l2_idx_share) = client.query_server(idx, server_id, &mut rng);
        // let (c_st, commit_rnds) = client._query_commit_rnd(&mut rng);
        // let rnd_share = commit_rnds[server_id];

        let resp = server.prc_protocol(query).await;



        let end_wall_duration = wall_timer.elapsed();
        let end_cpu_duration = get_cpu_time() - cpu_timer;
        let stat = server.get_then_reset_netstat().await;
        let (bt_used, dabit_use) = server.get_preproc_stat();


        log::info!(
            "# PIR query with {} party on server {} and db size {:?} => latency: {:?}. CPU Time: {:?}, Bandwidth: {}, preproc used: (bt: {}, dabit: {})",
            party_num,
            server_id,
            client.total_db_size(),
            end_wall_duration,
            end_cpu_duration,
            stat,
            bt_used,
            dabit_use
        );
        // party id, num parties, db l1, db l2, wall time, comp time, upload (byte), download (bytes)
        let log = format!(
            "{}, {}, {}, {}, {:?}, {:?}, {}, {}\n",
            cli_args.id,
            party_num,
            server.db.get_dims().0,
            server.db.get_dims().1,
            end_wall_duration,
            end_cpu_duration,
            stat.get_sent(),
            stat.get_received(),
        );



        if let Some(mut token) = resp{

            let check_rec_0 =token.check_for_rec_val(&st,0, &server.pk);
            let check_rec_1 =token.check_for_rec_val(&st,1, &server.pk);

            log::info!("Received token. accept check:{}, reject check:{}.", check_rec_0, check_rec_1);

            // We expect the value to be either accept or reject and the signature to be valid
            assert!(token.verify(&server.pk).is_ok()) 
            
            // log::info!("Received accept token. {}.", check_rec_0, check_rec_1);
            // let (idx_scalar, rnd) = (Scalar::from(idx as u32), crnd.as_scalar());
            // let check_rec_0 = commit.verify(&Opening::new(rnd, idx_scalar, Scalar::from(0u8)));
            // let check_rec_1 = commit.verify(&Opening::new(rnd, idx_scalar, Scalar::from(1u8)));

            // log::info!("Received commitment. Validity: [rec=0]: {}, [rec=1]: {}.", check_rec_0, check_rec_1);
            // log::debug!("Commitment. {:?}.", commit);

            // if !check_rec_0 && !check_rec_1{
            //     panic!("Received invalid commitment output that does not open.")
            // }
            // if check_rec_0 && check_rec_1{
            //     panic!("Commitment opened to 2 values!!! Should never run")
            // }
        }

        if let Some(fs) = measure_fs.as_mut() {
            fs.write_all(log.as_bytes())
                .expect("Failed to write measurement results to file");
        }
    }
}
