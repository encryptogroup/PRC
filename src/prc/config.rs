use futures::future::join_all;

use crate::{
    cor_rnd::{BeaverProvider, DaBitProvider},
    prc::{client::PRCClient, server::PRCServer},
};

/// Represents a peer in the multiparty computation system.
/// Each peer has an ID, IP address, port, and a seed used for correlated randomness generation.
/// The seed can be replaced with reading randomness from a file or other sources.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: usize,
    pub ip_addr: String,
    pub port: u16,
    pub seed: [u8; 32],
}

/// PRCDatabase configuration.
#[derive(Debug, Clone)]
pub struct DBConfig {
    db_record_size: usize,
    l1_dim: usize,
    l2_dim: usize,

    db_seed: Option<[u8; 32]>,
}

impl DBConfig {
    //// Creates a new DBConfig with the given parameters.
    ///
    /// # Panics
    /// Following are checks when trying to create a database and will panic if:
    /// Panics if record_size is larger than 32 bits.
    /// Panics if dimensions are not a power of two.
    /// Panics if l1_dim is smaller than l2_dim.
    /// Panics if l1_dim is smaller than 8.
    pub fn new(
        db_record_size: usize,
        l1_dim: usize,
        l2_dim: usize,
        db_seed: Option<[u8; 32]>,
    ) -> Self {
        DBConfig {
            db_record_size,
            l1_dim,
            l2_dim,
            db_seed,
        }
    }
    #[inline]
    pub fn db_record_size(&self) -> usize {
        self.db_record_size
    }
    #[inline]
    pub fn l1_dim(&self) -> usize {
        self.l1_dim
    }
    #[inline]
    pub fn l2_dim(&self) -> usize {
        self.l2_dim
    }
    #[inline]
    pub fn get_max_size(&self) -> usize {
        self.l1_dim * self.l2_dim
    }
    #[inline]
    pub fn db_seed(&self) -> &[u8; 32] {
        self.db_seed.as_ref().expect("DB seed is not set")
    }

    // pub fn query(&self, idx: usize) -> (BooleanValue, BooleanValue) {
    //     let (dim1, dim2) = self.get_logdim();
    //     let l1_share = BooleanValue::new(self.l1_dim as u8, l1 as u32);
    //     let l2_share = BooleanValue::new(self.l2_dim as u8, l2 as u32);
    //     (l1_share, l2_share)
    // }
}

#[derive(Debug, Clone)]
pub struct SystemConfig {
    pub peers: Vec<PeerInfo>,
    pub db_config: DBConfig,
}

impl SystemConfig {
    pub fn new(peers: Vec<PeerInfo>, db_config: DBConfig) -> Self {
        SystemConfig { peers, db_config }
    }

    /// returns the peer config information for the given peer id.
    #[inline]
    pub fn get_peer_info(&self, id: usize) -> Option<&PeerInfo> {
        self.peers.iter().find(|&peer| peer.id == id)
    }
    /// returns the peer config information for the given peer id.
    #[inline]
    pub fn get_peer_idx(&self, id: usize) -> Option<usize> {
        self.peers.iter().position(|peer| peer.id == id)
    }

    /// Returns the number of peers in the system.
    #[inline]
    pub fn get_party_num(&self) -> usize {
        self.peers.len()
    }

    /// Initializes correlated randomness providers for the system.
    /// The expected_query_num parameter is used to estimate the number of beaver triplets and
    /// DaBits needed for the computation.
    pub fn init_seeded_correlated_randomness(
        &self,
        id: usize,
        expected_query_num: usize,
    ) -> (BeaverProvider, DaBitProvider) {
        let beaver_per_q = (2.5 * self.db_config.l2_dim as f64).ceil() as usize + // 1.5 l2-bits for ohe + 1 l2-bit for PIR
            (1.5 * self.db_config.l1_dim as f64).ceil() as usize + // 1.5 l1-bits for ohe
            3 * 32; // Possible wastes for simd alignments during the computation
        let beaver_max_bitsize = expected_query_num * beaver_per_q;
        let dabit_max_size = expected_query_num * 36;

        let beaver_provider = self.gen_beaver_provider(id, beaver_max_bitsize);
        let dabit_provider = self.gen_dabit_provider(id, dabit_max_size);

        (beaver_provider, dabit_provider)
    }

    fn gen_beaver_provider(&self, id: usize, bit_num: usize) -> BeaverProvider {
        let mut beaver_provider = BeaverProvider::new();
        if id == 0 {
            let others_seed: Vec<&[u8; 32]> = self.peers.iter().skip(1).map(|p| &p.seed).collect();
            beaver_provider.generate_as_dealer(bit_num, &self.peers[0].seed, others_seed);
        } else {
            beaver_provider.generate_with_seed(bit_num, &self.peers[id].seed);
        }
        beaver_provider
    }

    fn gen_dabit_provider(&self, id: usize, bit_num: usize) -> DaBitProvider {
        let mut dabit_provider = DaBitProvider::new();
        if id == 0 {
            let others_seed: Vec<&[u8; 32]> = self.peers.iter().skip(1).map(|p| &p.seed).collect();
            dabit_provider.generate_as_dealer(bit_num, &self.peers[0].seed, others_seed);
        } else {
            dabit_provider.generate_with_seed(bit_num, &self.peers[id].seed);
        }
        dabit_provider
    }
}

/// Create a base configuration for n peers all running on localhost.
pub fn localhost_peer_config(party_num: usize, base_port: u16) -> Vec<PeerInfo> {
    let mut peers = Vec::new();
    for i in 0..party_num {
        let peer = PeerInfo {
            id: i,
            ip_addr: "127.0.0.1".to_string(),
            port: base_port + i as u16,
            seed: [(100 + i) as u8; 32], // SECURITY ISSUE: peers are getting all dealers share here.
                                         // This code aims to benchmark and do not handle secure dealer exchange and seed management.
        };
        peers.push(peer);
    }
    peers
}
/// create a basic system configuration for running all servers on localhost.
pub fn basic_system_config(party_num: usize, base_port: u16, db_dims: (usize, usize)) -> SystemConfig {
    SystemConfig {
        peers: localhost_peer_config(party_num, base_port),
        db_config: DBConfig {
            db_record_size: 1, // Default record size
            l1_dim: db_dims.1,
            l2_dim: db_dims.0,
            db_seed: Some([42; 32]), // Default seed
        },
    }
}

/// Initialize a full system running all server on a localhost with tcp connections.
pub async fn full_setup(
    party_num: usize,
    base_port: u16,
    db_dims: (usize, usize),
    max_req_num: usize,
) -> (PRCClient, Vec<PRCServer>) {
    let config = basic_system_config(party_num, base_port, db_dims);

    let servers = join_all((0..party_num).map(|server_id| {
        let config = config.clone();
        async move { PRCServer::new(server_id, config, max_req_num).await }
    }))
    .await;
    let client = PRCClient::new(party_num , config.db_config.clone(), config.clone());
    (client, servers)
}
