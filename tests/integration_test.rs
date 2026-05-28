use std::sync::Arc;

use priv_rec_cert::cor_rnd::{ BeaverProvider, BooleanValue, DaBitProvider};
use priv_rec_cert::prc::client::PRCClient;
use priv_rec_cert::prc::config::{basic_system_config, full_setup};
use priv_rec_cert::prc::connection::{ChannelManager, MpcMessageHandler};
use priv_rec_cert::prc::layer::LayerSource;
use priv_rec_cert::prc::mpc::MpcProvider;
use priv_rec_cert::prc::server::PRCServer;
use priv_rec_cert::prc::util::{
    reconstruct_arith, reconstruct_boolean, reconstruct_u32, secret_share_boolean,
    secret_share_lbit, 
};
use priv_rec_cert::simd_array::BitArray;

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Instant;

use std::sync::Once;

static INIT: Once = Once::new();

fn init_logger() {
    INIT.call_once(|| {
        env_logger::builder()
            .is_test(true) // important: prevents env_logger from clobbering test output
            .try_init()
            .ok();
    });
}

fn setup_n_party_mocked(
    n: usize,
    max_byte_size: usize,
) -> Vec<Arc<tokio::sync::Mutex<MpcProvider>>> {
    // Create a vector of seeds for each party
    let seeds: Vec<[u8; 32]> = (0..n).map(|i| [(i + 1) as u8; 32]).collect();
    let mut beaver_providers: Vec<BeaverProvider> = (0..n).map(|_| BeaverProvider::new()).collect();
    // Initialize the first BeaverProvider as the dealer
    beaver_providers[0].generate_as_dealer(
        max_byte_size * 8,
        &seeds[0],
        seeds[1..].iter().collect(),
    );
    // Initialize the rest of the BeaverProviders with their respective seeds
    for (i, provider) in beaver_providers.iter_mut().enumerate().skip(1) {
        provider.generate_with_seed(max_byte_size * 8, &seeds[i]);
    }
    log::debug!("Beaver providers initialized");

    // Create a channel connection between two peers
    let mut connection_factory = ChannelManager::new(n);

    let con_handler: Vec<MpcMessageHandler> =
        (0..n).map(|i| connection_factory.get_handler(i)).collect();
    log::debug!("Connection handlers initialized");

    let providers: Vec<Arc<tokio::sync::Mutex<MpcProvider>>> = beaver_providers
        .into_iter()
        .zip(con_handler)
        .enumerate()
        .map(|(i, (beaver_provider, handler))| {
            Arc::new(tokio::sync::Mutex::new(MpcProvider::new(
                i == 0, // Assume the first party is the dealer
                beaver_provider,
                DaBitProvider::new(),
                handler,
            )))
        })
        .collect();
    log::debug!("MpcProviders initialized");

    providers
}

pub async fn run_providers(providers: &[Arc<tokio::sync::Mutex<MpcProvider>>]) {
    let handles: Vec<_> = providers
        .iter()
        .enumerate()
        .map(|(i, provider)| {
            let provider_clone = provider.clone();
            tokio::spawn(async move {
                log::debug!(" -- Running provider {}", i);
                provider_clone.lock().await.run().await;
            })
        })
        .collect();
    futures::future::try_join_all(handles).await.unwrap();
    log::debug!(" -- providers run completed");
}

pub async fn get_reconstructed_result(
    providers: &[Arc<tokio::sync::Mutex<MpcProvider>>],
    z_sources: Vec<LayerSource>,
) -> BitArray {
    // Retrieve the results from both providers
    let results: Vec<_> = providers
        .iter()
        .zip(z_sources.into_iter())
        .map(|(provider, z_src)| {
            let provider_clone = provider.clone();
            async move { provider_clone.lock().await.get_output(z_src) }
        })
        .collect();

    let results = futures::future::join_all(results).await;
    let rec = BitArray::reconstruct(results);
    log::debug!("Results retrieved from providers");
    rec
}

#[tokio::test]
async fn test_mpc_nparty_ohe() {
    // Instantiate three MpcProviders with the connections
    init_logger();
    const N_PARTIES: usize = 3;
    let providers = setup_n_party_mocked(N_PARTIES, 1024);
    log::debug!("MpcProviders initialized");

    let mut rng = ChaCha8Rng::from_seed([42; 32]);

    // Generate ground truth arrays
    let array_lg: u8 = 8;
    let array_size = 1 << array_lg;
    // let idx = (rng.next_u32() % array_size) as u32;
    let idx = 3; // Fixed index for testing
    let mut expected = BitArray::new(array_size);
    expected.set_bit(idx as usize, true);

    // Secret share a and b for two parties
    let idx_sh = secret_share_lbit(idx, N_PARTIES, array_lg as usize, &mut rng);
    log::debug!("Input arrays generated and secret shared");

    {
        // Verify that the secret shares reconstruct to the original arrays
        let idx_rec = reconstruct_u32(idx_sh.clone());
        assert!(idx_rec == idx, "Reconstructed x does not match original");
    }

    let z_sources = idx_sh
        .into_iter()
        .enumerate()
        .map(|(i, idx_sh)| {
            let provider_clone = providers[i].clone();
            async move {
                provider_clone
                    .lock()
                    .await
                    .ohe_vec(BooleanValue::new(array_lg, idx_sh))
            }
        })
        .collect::<Vec<_>>();
    let z_sources = futures::future::join_all(z_sources).await;
    log::debug!("Output LayerSource created");

    run_providers(&providers).await;
    let result = get_reconstructed_result(&providers, z_sources).await;

    // Assert the results are as expected
    assert_eq!(
        result, expected,
        "OHE({}) failed in mocked channel MPC ",
        idx
    );
}

#[tokio::test]
async fn test_mpc_nparty_2ohe() {
    init_logger();
    // Instantiate three MpcProviders with the connections
    const N_PARTIES: usize = 2;
    let providers = setup_n_party_mocked(N_PARTIES, 16024);
    log::debug!("MpcProviders initialized");

    let mut rng = ChaCha8Rng::from_seed([42; 32]);

    // Generate ground truth arrays
    let array_lg1 = 13;
    let array_lg2 = 11;
    let array_size1 = 1 << array_lg1;
    let array_size2 = 1 << array_lg2;
    // let idx = (rng.next_u32() % array_size) as u32;
    let idx1 = 5; // Fixed index for testing
    let idx2 = 120; // Fixed index for testing
    let mut expected1 = BitArray::new(array_size1);
    let mut expected2 = BitArray::new(array_size2);
    expected1.set_bit(idx1 as usize, true);
    expected2.set_bit(idx2 as usize, true);

    // Secret share a and b for two parties
    let idx_sh1 = secret_share_lbit(idx1, N_PARTIES, array_lg1 as usize, &mut rng);
    let idx_sh2 = secret_share_lbit(idx2, N_PARTIES, array_lg2 as usize, &mut rng);
    log::debug!("Input arrays generated and secret shared");

    let z_sources1 = idx_sh1
        .into_iter()
        .enumerate()
        .map(|(i, idx_sh1)| {
            let provider_clone = providers[i].clone();
            async move {
                provider_clone
                    .lock()
                    .await
                    .ohe_vec(BooleanValue::new(array_lg1, idx_sh1))
            }
        })
        .collect::<Vec<_>>();
    let z_sources1 = futures::future::join_all(z_sources1).await;
    let z_sources2 = idx_sh2
        .into_iter()
        .enumerate()
        .map(|(i, idx_sh2)| {
            let provider_clone = providers[i].clone();
            async move {
                provider_clone
                    .lock()
                    .await
                    .ohe_vec(BooleanValue::new(array_lg2, idx_sh2))
            }
        })
        .collect::<Vec<_>>();
    let z_sources2 = futures::future::join_all(z_sources2).await;
    log::debug!("Output LayerSource created");

    run_providers(&providers).await;
    let result1 = get_reconstructed_result(&providers, z_sources1).await;
    let result2 = get_reconstructed_result(&providers, z_sources2).await;

    // Assert the results are as expected
    assert_eq!(
        result1, expected1,
        "OHE({}) failed in mocked channel MPC ",
        idx1
    );
    assert_eq!(
        result2, expected2,
        "OHE({}) failed in mocked channel MPC ",
        idx2
    );
}

#[tokio::test]
async fn test_mpc_nparty_single_layer() {
    init_logger();
    // Instantiate three MpcProviders with the connections
    const N_PARTIES: usize = 3;
    let providers = setup_n_party_mocked(N_PARTIES, 1024);
    log::debug!("MpcProviders initialized");

    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    let bit_size = 256 + 32;

    // Generate ground truth arrays
    // let x = BitArray::new(bit_size,);
    // let y = BitArray::new(bit_size,);
    let x = BitArray::random(bit_size, &mut rng);
    let y = BitArray::random(bit_size, &mut rng);
    let expected = BitArray::and(&x, &y);

    // Secret share a and b for two parties
    let xsh = x.secret_share(N_PARTIES, &mut rng);
    let ysh = y.secret_share(N_PARTIES, &mut rng);
    log::debug!("Input arrays generated and secret shared");

    {
        // Verify that the secret shares reconstruct to the original arrays
        let x_rec = BitArray::reconstruct(xsh.clone());
        let y_rec = BitArray::reconstruct(ysh.clone());
        assert!(x_rec == x, "Reconstructed x does not match original");
        assert!(y_rec == y, "Reconstructed y does not match original");
    }

    let z_sources = xsh
        .into_iter()
        .zip(ysh.into_iter())
        .enumerate()
        .map(|(i, (x_part, y_part))| {
            let provider_clone = providers[i].clone();
            async move {
                provider_clone.lock().await.and(
                    bit_size,
                    LayerSource::Input(x_part),
                    LayerSource::Input(y_part),
                )
            }
        })
        .collect::<Vec<_>>();
    let z_sources = futures::future::join_all(z_sources).await;
    log::debug!("Output LayerSource created");

    let handles: Vec<_> = providers
        .iter()
        .enumerate()
        .map(|(i, provider)| {
            let provider_clone = provider.clone();
            tokio::spawn(async move {
                log::debug!(" -- Running provider {}", i);
                provider_clone.lock().await.run().await;
            })
        })
        .collect();
    futures::future::try_join_all(handles).await.unwrap();
    log::debug!(" -- providers run completed");

    // Retrieve the results from both providers
    let results: Vec<_> = providers
        .iter()
        .zip(z_sources.into_iter())
        .map(|(provider, z_src)| {
            let provider_clone = provider.clone();
            async move { provider_clone.lock().await.get_output(z_src) }
        })
        .collect();

    let results = futures::future::join_all(results).await;

    let result = BitArray::reconstruct(results);
    log::debug!("Results retrieved from providers");

    // Assert the results are as expected
    assert_eq!(
        result, expected,
        "AND operation failed in mocked channel MPC with a single layer"
    );
}

#[tokio::test]
async fn test_mpc_with_2local_provider_single_layer() {
    init_logger();
    // Create a beaver providers
    let max_byte_size = 1024;
    let mut beaver_provider0 = BeaverProvider::new();
    let mut beaver_provider1 = BeaverProvider::new();
    beaver_provider0.generate_as_dealer(max_byte_size * 8, &[1; 32], vec![&[2; 32]]);
    beaver_provider1.generate_with_seed(max_byte_size * 8, &[2; 32]);
    log::debug!("Beaver providers initialized");

    // Create a channel connection between two peers
    let mut connection_factory = ChannelManager::new(2);
    let mut p0_conn_handler = MpcMessageHandler::new(0);
    let mut p1_conn_handler = MpcMessageHandler::new(1);

    let con0 = Arc::new(tokio::sync::Mutex::new(
        connection_factory
            .get_single_connection(0, 1)
            .expect("Failed to get connection P(0 -> 1)"),
    ));
    let con1 = Arc::new(tokio::sync::Mutex::new(
        connection_factory
            .get_single_connection(1, 0)
            .expect("Failed to get connection P(1 -> 0)"),
    ));

    p0_conn_handler.add_peer(con0.clone());
    p1_conn_handler.add_peer(con1.clone());
    log::debug!("Connection handlers initialized");

    // Instantiate two MpcProviders with the connections
    let provider0 = Arc::new(tokio::sync::Mutex::new(MpcProvider::new(
        true,
        beaver_provider0,
        DaBitProvider::new(),
        p0_conn_handler,
    )));
    let provider1 = Arc::new(tokio::sync::Mutex::new(MpcProvider::new(
        false,
        beaver_provider1,
        DaBitProvider::new(),
        p1_conn_handler,
    )));
    log::debug!("MpcProviders initialized");

    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    let bit_size = 256 + 32;
    // Generate ground truth arrays
    // let x = BitArray::new(bit_size);
    // let y = BitArray::new(bit_size);
    let x = BitArray::random(bit_size, &mut rng);
    let y = BitArray::random(bit_size, &mut rng);
    let expected = BitArray::and(&x, &y);

    // Secret share a and b for two parties
    let mut x0 = x.clone();
    let mut y0 = y.clone();
    let x1 = x0.inplace_secret_share(2, &mut rng).pop().unwrap();
    let y1 = y0.inplace_secret_share(2, &mut rng).pop().unwrap();
    log::debug!("Input arrays generated and secret shared");

    let z0_src =
        provider0
            .lock()
            .await
            .and(bit_size, LayerSource::Input(x0), LayerSource::Input(y0));
    let z1_src =
        provider1
            .lock()
            .await
            .and(bit_size, LayerSource::Input(x1), LayerSource::Input(y1));
    log::debug!("Output LayerSource created");

    let provider0_clone = provider0.clone();
    let provider1_clone = provider1.clone();
    log::debug!("Providers cloned for concurrent execution");

    let handle1 = tokio::spawn(async move {
        log::debug!(" -- Running provider0");
        provider0_clone.lock().await.run().await;
    });

    let handle2 = tokio::spawn(async move {
        log::debug!(" -- Running provider1");
        provider1_clone.lock().await.run().await;
    });

    tokio::try_join!(handle1, handle2).unwrap();
    log::debug!(" -- providers run completed");

    // Retrieve the results from both providers
    let result0 = provider0.lock().await.get_output(z0_src);
    let result1 = provider1.lock().await.get_output(z1_src);
    log::debug!("Results retrieved from providers");

    let mut result = result0.clone();
    result.inplace_xor(&result1);

    // Assert the results are as expected
    assert_eq!(
        result, expected,
        "AND operation failed in mocked channel MPC with a single layer"
    );
}

#[tokio::test]
pub async fn prc_server_ohe() {
    init_logger();
    let config = basic_system_config(2, 8000, (128, 256));
    let mut connection_factory = ChannelManager::new(2);

    let mut server0 = PRCServer::new_with_message_handler(
        0,
        config.clone(),
        connection_factory.get_handler(0),
        2,
    );
    let mut server1 = PRCServer::new_with_message_handler(
        1,
        config.clone(),
        connection_factory.get_handler(1),
        2,
    );

    let mut expected = BitArray::new(1 << 5);
    expected.set_bit(13, true);
    let handlers = vec![
        server0.ohe(BooleanValue::new(5, 4)),
        server1.ohe(BooleanValue::new(5, 9)),
    ];
    let ans = futures::future::join_all(handlers).await;

    let ans = BitArray::reconstruct(ans);
    assert_eq!(ans, expected, "OHE result does not match expected value");

    let mut expected = BitArray::new(1 << 6);
    expected.set_bit(12, true);
    let handlers = vec![
        server0.ohe(BooleanValue::new(6, 4)),
        server1.ohe(BooleanValue::new(6, 8)),
    ];
    let ans = futures::future::join_all(handlers).await;

    let ans = BitArray::reconstruct(ans);
    assert_eq!(ans, expected, "OHE result does not match expected value");
}

#[tokio::test]
pub async fn prc_server_rec_retrieve() {
    init_logger();
    const REP_NUM: usize = 100;
    let config = basic_system_config(2, 8000, (1024, 2048));
    let client = PRCClient::new(2, config.db_config.clone(), config.clone());

    let mut connection_factory = ChannelManager::new(2);
    let mut server0 = PRCServer::new_with_message_handler(
        0,
        config.clone(),
        connection_factory.get_handler(0),
        REP_NUM,
    );
    let mut server1 = PRCServer::new_with_message_handler(
        1,
        config.clone(),
        connection_factory.get_handler(1),
        REP_NUM,
    );

    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    let start = Instant::now();
    for _ in 0..REP_NUM {
        let idx = rng.next_u32() as usize % client.total_db_size();
        // let idx = 5;
        let expected = server0.db.get_record_bit(idx, 0);
        let (idx_l1_sh, idx_l2_sh) = client._query_index(idx, &mut rng);

        {
            // Verify that the secret shares reconstruct to the original indices
            let idx_l1_rec = reconstruct_boolean(&idx_l1_sh);
            let idx_l2_rec = reconstruct_boolean(&idx_l2_sh);
            let (idx_l1, idx_l2) = client.idx_to_2dim(idx);

            assert!(
                idx_l1_rec == idx_l1 as u32,
                "Reconstructed idx_l1 does not match original"
            );
            assert!(
                idx_l2_rec == idx_l2 as u32,
                "Reconstructed idx_l2 does not match original"
            );
        }

        let handlers = vec![
            server0.rec_retrieve(idx_l1_sh[0], idx_l2_sh[0]),
            server1.rec_retrieve(idx_l1_sh[1], idx_l2_sh[1]),
        ];
        let ans = futures::future::join_all(handlers).await;

        log::debug!("PIR answer shares: {:?}", ans);
        let ans = ans.iter().fold(false, |acc, &s| acc ^ s);

        let (idx_l1, idx_l2) = client.idx_to_2dim(idx);
        log::debug!("# PIR answer recons : {:?}", ans);
        log::debug!(
            "Expected lvl1 answer:\n ### {:?}",
            server0.db._lvl1_plain_idx(idx_l1)
        );
        log::debug!(
            "PIR retrieval for index ({}, {}) returned: {}",
            idx_l1,
            idx_l2,
            ans
        );

        assert_eq!(
            ans, expected,
            "PIR retrieval failed for index ({}, {})",
            idx_l1, idx_l2
        );
    }
    log::info!(
        " $$$$ 10 PIR retrieval of DB size {} completed in {:?}",
        server0.db.total_size(),
        start.elapsed()
    );
}

#[tokio::test]
pub async fn prc_tcp_rec_retrieve() {
    init_logger();

    let party_num: usize = 2;
    let db_size = (8 * 1024, 8 * 1024); // (dim1, dim2)
    let rep = 10;
    let (client, mut servers) = full_setup(party_num, 8020, db_size, rep).await;

    // ask rep
    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    let finished_init = Instant::now();
    for _ in 0..rep {
        let idx = rng.next_u32() as usize % client.total_db_size();
        let expected = servers[0].db.get_record_bit(idx, 0);
        let (idx_l1_sh, idx_l2_sh) = client._query_index(idx, &mut rng);

        let handlers: Vec<_> = servers
            .iter_mut()
            .enumerate()
            .map(|(i, server)| server.rec_retrieve(idx_l1_sh[i], idx_l2_sh[i]))
            .collect();

        let ans = futures::future::join_all(handlers).await;
        let ans: bool = ans.iter().fold(false, |acc, &s| acc ^ s);

        for server in servers.iter_mut() {
            let stat = server.get_then_reset_netstat().await;
            log::info!(
                "Server {} have used {} when performing PIR over DB {:?}.",
                server.id,
                stat,
                client.get_dims(),
            );
        }

        log::debug!(
            "# PIR query {} -> {}. Expected answer: {}",
            idx,
            ans,
            expected
        );
        assert_eq!(ans, expected, "PIR retrieval failed for index ({})", idx);
    }

    log::info!(
        " $$$$ {} PIR retrieval of DB size {} completed in {:?}. ",
        rep,
        client.total_db_size(),
        finished_init.elapsed(),
    );
}

#[tokio::test]
pub async fn prc_tcp_full() {
    init_logger();

    let party_num: usize = 2;
    let db_size = (8 * 1024, 8 * 1024); // (dim1, dim2)
    let rep = 10;
    let (client, mut servers) = full_setup(party_num, 8010, db_size, rep).await;
    let verifyier_key = servers[0].pk;
    // let db = PRCDatabase::from_config(&config.db_config);

    // ask rep
    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    let finished_init = Instant::now();
    for _ in 0..rep {
        let idx = rng.next_u32() as usize % client.total_db_size();
        let expected = servers[0].db.get_record_bit(idx, 0);

        let (st, queries) = client.query_all_servers(idx, &mut rng);

        let handlers: Vec<_> = servers
            .iter_mut()
            .enumerate()
            .map(|(i, server)| server.prc_protocol(queries[i].clone()))
            .collect();

        let token_list = futures::future::join_all(handlers).await;
        let mut token = token_list.into_iter().flatten().next().unwrap();
        

        for server in servers.iter_mut() {
            let stat = server.get_then_reset_netstat().await;
            log::info!(
                "Server {} have used {} when performing PIR over DB {:?}.",
                server.id,
                stat,
                client.get_dims(),
            );
        }

        let value_checks = token.check_for_rec_val(&st, expected as u32, &verifyier_key);
        assert!(value_checks);
        assert!(token.verify(&verifyier_key).is_ok());
        log::info!(" $$$$ Token verification succeeded. Value:{}, token:{:?}",value_checks, token);
    }

    log::info!(
        " $$$$ {} PIR retrieval of DB size {} completed in {:?}. ",
        rep,
        client.total_db_size(),
        finished_init.elapsed(),
    );
}

#[tokio::test]
pub async fn prc_tcp_b2a() {
    init_logger();

    let party_num = 2;
    let db_size = (8, 8); // Only checks the conversion and not db
    let rep = 10;
    let batch = 10;
    let (mut _client, mut servers) = full_setup(party_num, 8030, db_size, rep*batch).await;


    // ask rep
    let mut rng = ChaCha8Rng::from_seed([42; 32]);
    for _ in 0..rep {
        let bnum = 16;

        let vals = (0..batch)
            .map(|_| rng.next_u32() % (1 << bnum))
            .collect::<Vec<_>>();
        let expected = vals.clone();

        let vals_bool_sh = vals
            .iter()
            .map(|v| secret_share_boolean(BooleanValue::new(bnum, *v), party_num, &mut rng))
            .collect::<Vec<_>>();

        let handlers: Vec<_> = servers
            .iter_mut()
            .enumerate()
            .map(|(i, server)| {
                server.conv_b2a(vals_bool_sh.iter().map(|arr| arr[i]).collect::<Vec<_>>())
            })
            .collect();

        let ans = futures::future::join_all(handlers).await;

        log::debug!("arithmetic response for conv {:?}", ans);

        for i in 0..batch {
            let ans_i =
                reconstruct_arith(&ans.iter().map(|s| s[i]).collect::<Vec<_>>()).as_u32();
            log::info!(
                "# B2A conversion {} -> {}. Expected answer: {}",
                vals[i],
                ans_i,
                expected[i]
            );
            assert_eq!(
                ans_i, expected[i],
                "B2A conversion failed for value {}",
                vals[i]
            );
        }
    }
}

