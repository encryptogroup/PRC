use std::net::SocketAddr;

use crate::cor_rnd::{ArithValueT, BooleanValue, join_boolean};
use crate::prc::client::{PRCQuery, PRCToken};
use crate::prc::commitment::{Commitment, Opening};
use crate::prc::layer::LayerSource;
use crate::prc::{config::SystemConfig, connection::TcpConnectionFactory};
use crate::simd_array::BitArray;


use ed25519_dalek::ed25519::signature::SignerMut;
use rand::rngs::OsRng;
use ed25519_dalek::{SigningKey, VerifyingKey};
 

use super::{
    PRCDatabase,
    connection::{MpcMessageHandler, NetStat},
    mpc::MpcProvider,
};

pub struct PRCServer {
    pub id: usize,
    pub config: SystemConfig,
    pub db: PRCDatabase,
    pub pk: VerifyingKey,
    mpc_provider: MpcProvider,
    sk: SigningKey,
}


impl PRCServer {
    pub fn new_with_message_handler(
        id: usize,
        config: SystemConfig,
        message_handler: MpcMessageHandler,
        expected_query_num: usize,
    ) -> Self {
        // Initialize the database with the seed from the configuration
        let db = PRCDatabase::from_config(&config.db_config);

        let (beaver, dabit) = config.init_seeded_correlated_randomness(id, expected_query_num);
        let mpc_provider = MpcProvider::new(
            id == 0, // Party 0 handles constant additions
            beaver,
            dabit,
            message_handler,
        );

        let mut csprng = OsRng;
        let sk: SigningKey = SigningKey::generate(&mut csprng);
        let pk = sk.verifying_key();

        PRCServer {
            id,
            config,
            db,
            mpc_provider,
            sk,
            pk,
        }
    }

    pub async fn new(id: usize, config: SystemConfig, expected_query_num: usize) -> Self {
        // Establish the message handler for MPC communication
        let mut tcp_factory = TcpConnectionFactory::new(id, config.get_party_num());
        let self_peer_config = config.get_peer_info(id).expect("Peer info not found");
        let listen_add: SocketAddr = SocketAddr::new(
            self_peer_config
                .ip_addr
                .parse()
                .expect("Failed to parse IP address"),
            self_peer_config.port,
        );
        tcp_factory.listen(listen_add).await;
        log::info!("TCP listen on server {}.", id);
        tcp_factory.connect(&config.peers).await;
        log::info!("TCP connections established on server {}.", id);

        let message_handler = tcp_factory.get_handler().await;
        Self::new_with_message_handler(id, config, message_handler, expected_query_num)
    }

    pub async fn ohe(&mut self, idx_bshare: BooleanValue) -> BitArray {
        let ohe_src = self.mpc_provider.ohe_vec(idx_bshare);
        self.mpc_provider.run().await;

        let ohe_sh = self.mpc_provider.get_output(ohe_src);
        self.mpc_provider.clear(); // drop all old layers
        ohe_sh
    }

    pub async fn rec_retrieve(
        &mut self,
        l1_idx_share: BooleanValue,
        l2_idx_share: BooleanValue,
    ) -> bool {
        // let (dim_1, dim_2) = self.db.get_logdim();
        let start_time = std::time::Instant::now();
        let l1_ohe_src = self.mpc_provider.ohe_vec(l1_idx_share);
        let l2_ohe_src = self.mpc_provider.ohe_vec(l2_idx_share);

        self.mpc_provider.run().await;

        let ohe_time = std::time::Instant::now();
        log::info!(
            "server {} OHE time: {:?}",
            self.id,
            ohe_time.duration_since(start_time)
        );

        let l1_ohe = self.mpc_provider.get_output(l1_ohe_src);
        let l2_ohe = self.mpc_provider.get_output(l2_ohe_src);
        self.mpc_provider.clear(); // drop all old layers

        log::debug!(
            "server {} OHE out:\n --- l1: {:?}\n --- l2: {:?}\n",
            self.id,
            l1_ohe,
            l2_ohe
        );

        let l1_pir_ans = self.db.lvl1_pir_retrieve(&l1_ohe);

        let pir_l1_time = std::time::Instant::now();
        log::info!(
            "server {} PIR lvl1 time: {:?}",
            self.id,
            pir_l1_time.duration_since(ohe_time)
        );

        assert_eq!(l1_pir_ans.bit_size(), l2_ohe.bit_size());
        log::debug!(
            "server {} PIR out:\n *** l1_pir: {:?}\n *** l2_ohe: {:?}\n",
            self.id,
            l1_pir_ans,
            l2_ohe
        );

        let p2_ans_src = self.mpc_provider.and(
            l1_pir_ans.bit_size(),
            LayerSource::Input(l1_pir_ans),
            LayerSource::Input(l2_ohe),
        );
        self.mpc_provider.run().await;

        let pir_l2_time = std::time::Instant::now();
        log::info!(
            "server {} PIR lvl2: {:?}",
            self.id,
            pir_l2_time.duration_since(pir_l1_time)
        );

        let p2_ans = self.mpc_provider.get_output(p2_ans_src);
        log::debug!("server {} PIR out:\n *** l2_pir: {:?}\n", self.id, p2_ans);

        p2_ans.parity()
    }


    pub async fn prc_protocol(
        &mut self,
        query: PRCQuery
    ) -> Option<PRCToken> {
        let start = std::time::Instant::now();

        let record_share = self.rec_retrieve(query.l1_bss, query.l2_bss).await;

        let rec_ret: std::time::Instant = std::time::Instant::now();
        log::info!(
            "server {} record retrieval: {:?}",
            self.id,
            rec_ret.duration_since(start)
        );

        let arith = self
            .conv_b2a(vec![
                join_boolean(query.l1_bss, query.l2_bss),
                BooleanValue::new(1, record_share as u32),
            ])
            .await;

        let conv_time = std::time::Instant::now();
        log::info!(
            "server {} conversion: {:?}",
            self.id,
            conv_time.duration_since(rec_ret)
        );

        let opening = Opening::new(
            query.rnd_ass.as_scalar(), arith[0].as_scalar(), arith[1].as_scalar()
        );
        let peer_commit = Commitment::commit(&opening);
        let agg_commit = self.mpc_provider.aggregate_commitments(peer_commit).await;

        let commit_time = std::time::Instant::now();
        log::info!(
            "server {} commitment time: {:?}",
            self.id,
            commit_time.duration_since(conv_time)
        );

        let mut token: Option<PRCToken> = None;
        if let Some(commit) = agg_commit{
            token = Some(PRCToken { 
                commitment: None, 
                sig: self.sk.sign(&commit.serialize())
            })
        }
        let sig_time = std::time::Instant::now();
        log::info!(
            "server {} signing time: {:?}",
            self.id,
            sig_time.duration_since(commit_time)
        );

        log::debug!("server {} full protocol out: {:?}", self.id, token);
        
        token
    }

    pub async fn conv_b2a(&mut self, binaries: Vec<BooleanValue>) -> Vec<ArithValueT> {
        self.mpc_provider.run_conv_B2A(binaries).await
    }

    pub async fn get_then_reset_netstat(&mut self) -> NetStat {
        self.mpc_provider.get_then_reset_netstat().await
    }

    pub fn get_preproc_stat(&mut self) -> (usize, usize) {
        self.mpc_provider.get_preproc_stat()
    }

}


// pub fn verify_prc_token()