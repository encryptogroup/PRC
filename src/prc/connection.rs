use crate::prc::commitment::Commitment;
use crate::prc::config::PeerInfo;
use crate::simd_array::{BitArray, D2BitArray};

use core::panic;
use std::collections::HashMap;
use tokio::sync::mpsc::{self, Receiver, Sender};

use futures::future::join_all;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;


/// An abstraction for communication between two peers in the MPC protocol.
#[async_trait::async_trait]
pub trait PeerConnection: Send + Sync {
    async fn send(&self, cmd: &Cmd<'_>);
    async fn receive(&mut self) -> Option<Cmd<'_>>;
    fn set_peer_id(&mut self, id: usize);
    fn get_peer_id(&self) -> Option<usize>;
    async fn get_stat(&self) -> NetStat;
    async fn reset_stat(&mut self);
}

type ArcPeerConnection = Arc<Mutex<Box<dyn PeerConnection>>>;

/// Commands used in the MPC protocol.
#[derive(Debug, Clone)]
pub enum Cmd<'a> {
    Handshake {
        peer_id: u8,
    },
    BeaverExchange {
        sender_id: u8,
        beaver_bit_size: u32,
        beaver_data_a: &'a [u8],
        beaver_data_b: &'a [u8],
    },
    DabitExchange {
        sender_id: u8,
        dabit_bit_size: u32,
        dabit_bool_data: &'a [u8],
    },
    AggCommit{
        sender_id: u8,
        commit: Commitment,
    }
}

impl<'a> Cmd<'a> {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        match self {
            Cmd::BeaverExchange {
                sender_id,
                beaver_bit_size,
                beaver_data_a,
                beaver_data_b,
            } => {
                buffer.push(0u8); // Command type for BeaverExchange
                buffer.push(*sender_id);
                buffer.extend_from_slice(&beaver_bit_size.to_le_bytes());
                buffer.extend_from_slice(beaver_data_a);
                buffer.extend_from_slice(beaver_data_b);
            }
            Cmd::Handshake { peer_id } => {
                buffer.push(1u8); // Command type for Handshake
                buffer.push(*peer_id);
            }
            Cmd::DabitExchange {
                sender_id,
                dabit_bit_size,
                dabit_bool_data,
            } => {
                buffer.push(2u8); // Command type for DabitExchange
                buffer.push(*sender_id);
                buffer.extend_from_slice(&dabit_bit_size.to_le_bytes());
                buffer.extend_from_slice(dabit_bool_data);
            }
            Cmd::AggCommit {
                sender_id,
                commit
            } => {
                buffer.push(3u8); 
                buffer.push(*sender_id);
                buffer.extend_from_slice(&commit.serialize());
            }
        }
        buffer
    }

    pub fn deserialize(data: &'a [u8]) -> Cmd<'a> {
        let cmd_type = data[0];
        match cmd_type {
            0 => {
                let sender_id = data[1];
                let beaver_bit_size = u32::from_le_bytes(data[2..6].try_into().unwrap());
                let array_size = (beaver_bit_size as usize).div_ceil(8); // Convert bits to bytes
                let beaver_data_a = &data[6..6 + array_size];
                let beaver_data_b = &data[6 + array_size..6 + 2 * array_size];
                Cmd::BeaverExchange {
                    sender_id,
                    beaver_bit_size,
                    beaver_data_a,
                    beaver_data_b,
                }
            }
            1 => {
                let peer_id = data[1];
                Cmd::Handshake { peer_id }
            }
            2 => {
                let sender_id = data[1];
                let dabit_bit_size = u32::from_le_bytes(data[2..6].try_into().unwrap());
                let array_size = (dabit_bit_size as usize).div_ceil(8); // Convert bits to bytes
                let dabit_bool_data = &data[6..6 + array_size];
                Cmd::DabitExchange { 
                    sender_id,
                    dabit_bit_size,
                    dabit_bool_data,
                }
            }
            3 => {
                let sender_id = data[1];
                let commit = Commitment::deserialize( &data[2..34]);
                Cmd::AggCommit {sender_id, commit}
            }
            _ => panic!("Unknown command type: {}", cmd_type),
        }
    }
}

/// MpcMessageHandler is responsible for handling communication between peers in the MPC protocol.
pub struct MpcMessageHandler {
    owner_id: u8, // ID of the owner of this handler
    peers: Vec<ArcPeerConnection>,

    active_beaver_storage: Arc<Mutex<Vec<D2BitArray>>>,
    active_dabit_storage: Arc<Mutex<Vec<BitArray>>>,
    commit_storage: Arc<Mutex<Vec<Commitment>>>,
}

impl MpcMessageHandler {
    pub fn new(owner_id: u8) -> Self {
        MpcMessageHandler {
            owner_id,
            peers: Vec::new(),
            active_beaver_storage: Arc::new(Mutex::new(Vec::new())),
            active_dabit_storage: Arc::new(Mutex::new(Vec::new())),
            commit_storage: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn get_id(&self) -> u8 {
        self.owner_id
    }

    pub fn add_peer(&mut self, peer: ArcPeerConnection) {
        self.peers.push(peer);
    }

    pub fn is_aggregator(&self) -> bool{
        self.owner_id == 0
    }
    pub fn get_aggregator(&self) -> ArcPeerConnection {
        assert!( !self.is_aggregator() );
        self.peers[0].clone()
    }

    pub async fn send_beaver_shares(&self, beaver: D2BitArray) {
        let a = beaver.get_array(0);
        let b = beaver.get_array(1);
        let cmd = Cmd::BeaverExchange {
            sender_id: self.owner_id,
            beaver_bit_size: a.bit_size() as u32,
            beaver_data_a: a.get_inner_memref(),
            beaver_data_b: b.get_inner_memref(),
        };
        self.broadcast(cmd).await;
    }

    pub async fn send_dabit_shares(&self, dabit: BitArray) {
        let cmd = Cmd::DabitExchange { 
            sender_id: self.owner_id,
            dabit_bit_size: dabit.bit_size() as u32,
            dabit_bool_data: dabit.get_inner_memref(),
        };
        self.broadcast(cmd).await;
    }

    /// Broadcast commitment for aggregation
    pub async fn send_commit_to_agg(&self, commit:Commitment) {
        let cmd = Cmd::AggCommit { 
            sender_id: self.owner_id, 
            commit,
        };
        self.broadcast(cmd).await;
        // let aggregator = self.get_aggregator();
        // aggregator.lock().await.send(&cmd).await; 
    }


    pub async fn receive_all(&mut self) {
        // Collect futures for receiving commands from all peers
        let receive_futures = self.peers.iter().map(|peer| async {
            if let Some(cmd) = peer.lock().await.receive().await {
                self.handle_cmd(cmd).await;
            }
        });
        futures::future::join_all(receive_futures).await;
    }
    

    pub async fn receive_beaver_shares(&mut self) -> Vec<D2BitArray> {
        self.receive_all().await;

        // Check if we have received enough beaver shares
        let mut storage = self.active_beaver_storage.lock().await;

        assert!(
            storage.len() == self.peers.len(),
            "Expected {} beaver shares, but got {}. Currently, concurrent PRC requests are not supported.",
            self.peers.len(),
            storage.len()
        );

        // Collect all beaver shares and reset the storage
        storage.drain(..).collect()
    }
    
    

    pub async fn receive_dabit_shares(&mut self) -> Vec<BitArray> {
        self.receive_all().await;

        // Check if we have received enough dabit shares
        let mut storage = self.active_dabit_storage.lock().await;

        assert!(
            storage.len() == self.peers.len(),
            "Expected {} dabit shares, but got {}. Currently, concurrent PRC requests are not supported.",
            self.peers.len(),
            storage.len()
        );
        // Collect all beaver shares and reset the storage
        
        storage.drain(..).collect()
    }

    pub async fn receive_agg_commits(&mut self) -> Vec<Commitment>{
        self.receive_all().await;
        let mut storage = self.commit_storage.lock().await;

        // log::info!("Receiving aggregate commits with: storage len:{}, peers len: {}",  storage.len(), self.peers.len());

        assert!(
            storage.len() == self.peers.len() , // Add 1 for the aggregator's own commit
            "Expected {} commitments to aggregate but only found {}.",
            self.peers.len(),
            storage.len()
        );

        storage.drain(..).collect()
    }

    /// Broadcast a command to all peers.
    /// This will send the command to all connected peers asynchronously.
    pub async fn broadcast(&self, cmd: Cmd<'_>) {
        let arc_cmd = Arc::new(cmd);
        let futures = self.peers.iter().map(|peer| {
            let arc_cmd = Arc::clone(&arc_cmd);
            async move {
                let _ = peer.lock().await.send(&arc_cmd).await;
            }
        });

        join_all(futures).await;
    }



    pub async fn handle_cmd(&self, cmd: Cmd<'_>) {
        match cmd {
            Cmd::AggCommit {sender_id,  commit } => {
                assert!(
                    sender_id != self.owner_id,
                    "Should not receive messages from self. Sender ID: {}, Owner ID: {}",
                    sender_id,
                    self.owner_id
                );
                let mut storage = self.commit_storage.lock().await;
                storage.push(commit);
            }
            Cmd::BeaverExchange {
                sender_id,
                beaver_bit_size,
                beaver_data_a,
                beaver_data_b,
            } => {
                assert!(
                    sender_id != self.owner_id,
                    "Should not receive messages from self. Sender ID: {}, Owner ID: {}",
                    sender_id,
                    self.owner_id
                );
                let a = BitArray::from_byte_slice(beaver_data_a, beaver_bit_size as usize);
                let b = BitArray::from_byte_slice(beaver_data_b, beaver_bit_size as usize);
                let beaver: D2BitArray = D2BitArray::new(vec![a, b]);

                let mut storage = self.active_beaver_storage.lock().await;
                storage.push(beaver);
            }
            Cmd::DabitExchange {
                sender_id,
                dabit_bit_size,
                dabit_bool_data,
            } => {
                assert!(
                    sender_id != self.owner_id,
                    "Should not receive messages from self. Sender ID: {}, Owner ID: {}",
                    sender_id,
                    self.owner_id
                );
                let e = BitArray::from_byte_slice(dabit_bool_data, dabit_bit_size as usize);

                let mut storage = self.active_dabit_storage.lock().await;
                storage.push(e);
            }
            Cmd::Handshake { peer_id } => {
                panic!(
                    "Did not expect to receive handshake at this stage. peer_id: {}",
                    peer_id
                );
            }
        }
    }

    pub async fn get_then_reset_netstat(&mut self) -> NetStat{
        let mut out = NetStat {
            sent: 0,
            received: 0,
        };
        for peer in &self.peers {
            let stat = peer.lock().await.get_stat().await;
            out = out.add(stat);
            peer.lock().await.reset_stat().await;
        }
        out
    }
}

/// NetStat is a struct that keeps track of the network statistics.
#[derive(Clone, Debug)]
pub struct NetStat{
    sent: u32,
    received: u32,
}
impl NetStat {
    pub fn new_arc() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(NetStat {
            sent: 0,
            received: 0,
        }))
    }
    fn add(self, other: Self) -> Self {
        NetStat {
            sent: self.sent + other.sent,
            received: self.received + other.received,
        }
    }

    ///returns the total number of bytes sent
    pub fn get_sent(&self) -> u32 {
        self.sent
    }
    ///returns the total number of bytes received
    pub fn get_received(&self) -> u32 {
        self.received
    }
}
impl std::fmt::Display for NetStat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NetStat {{ sent: {}, received: {} }}", self.sent, self.received)
    }
}


/// This struct creates a mock connection between peers using channels.
/// This is intended for testing purposes only.
/// It simulates the behavior of a peer-to-peer connection without actual network communication.
/// One managers handles all peer and generates $n MpcMessageHandler for each peer.
pub struct ChannelManager {
    num_peers: usize,
    sender_channels: HashMap<(usize, usize), Sender<Vec<u8>>>,
    receiver_channels: HashMap<(usize, usize), Receiver<Vec<u8>>>,
}

impl ChannelManager {
    pub fn new(num_peers: usize) -> Self {
        let mut sender_channels = HashMap::new();
        let mut receiver_channels = HashMap::new();

        for a in 0..num_peers {
            for b in 0..num_peers {
                if a != b {
                    let (tx, rx) = mpsc::channel(100);
                    sender_channels.insert((a, b), tx);
                    receiver_channels.insert((b, a), rx);
                }
            }
        }

        ChannelManager {
            num_peers,
            sender_channels,
            receiver_channels,
        }
    }

    pub fn get_single_connection(&mut self, a: usize, b: usize) -> Option<Box<dyn PeerConnection>> {
        let tx = self.sender_channels.remove(&(a, b))?;
        let rx = self.receiver_channels.remove(&(a, b))?;
        Some(Box::new(ChannelConnection {
            peer_id: Some(b),
            sender: tx,
            receiver: rx,
            stat: NetStat::new_arc(),
        }))
    }

    pub fn get_handler(&mut self, peer_id: usize) -> MpcMessageHandler {
        let mut con_handler = MpcMessageHandler::new(peer_id as u8);
        for i in 0..self.num_peers {
            if i == peer_id {
                continue; // Skip self
            }
            let con_to_i = Arc::new(tokio::sync::Mutex::new(
                self.get_single_connection(peer_id, i)
                    .unwrap_or_else(|| panic!("Failed to get connection P({} -> {})", peer_id, i)),
            ));
            con_handler.add_peer(con_to_i.clone());
        }
        con_handler
    }
}

pub struct ChannelConnection {
    peer_id: Option<usize>,
    stat: Arc<Mutex<NetStat>>,
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
}

#[async_trait::async_trait]
impl PeerConnection for ChannelConnection {
    async fn send(&self, cmd: &Cmd<'_>) {
        let serialized: Vec<u8> = cmd.serialize();
        self.stat.lock().await.sent += serialized.len() as u32;
        let _ = self.sender.send(serialized).await;
    }

    async fn receive(&mut self) -> Option<Cmd<'_>> {
        let data = self.receiver.recv().await?;
        self.stat.lock().await.received += data.len() as u32;
        let deserialized = Cmd::deserialize(Box::leak(data.into_boxed_slice()));
        return Some(deserialized);
    }

    fn set_peer_id(&mut self, id: usize) {
        self.peer_id = Some(id);
    }
    fn get_peer_id(&self) -> Option<usize> {
        self.peer_id
    }
    async fn get_stat(&self) -> NetStat {
        let stat = self.stat.lock().await;
        return NetStat {
            sent: stat.sent,
            received: stat.received,
        }
    }
    async fn reset_stat(&mut self) {
        let mut stat = self.stat.lock().await;
        stat.sent = 0;
        stat.received = 0;
    }
}

pub struct TcpConnection {
    stat: Arc<Mutex<NetStat>>,
    receiver_id: Option<usize>,
    stream: Arc<Mutex<TcpStream>>,
}

#[async_trait::async_trait]
impl PeerConnection for TcpConnection {
    async fn send(&self, cmd: &Cmd<'_>) {
        let serialized = cmd.serialize();
        self.stat.lock().await.sent += serialized.len() as u32 + 4; // 4 bytes for length prefix
        let mut stream = self.stream.lock().await;
        let _ = stream.write_u32_le(serialized.len() as u32).await;
        let _ = stream.write_all(&serialized).await;
    }

    async fn receive(&mut self) -> Option<Cmd<'_>> {
        let mut stream = self.stream.lock().await;
        let cmd_len = stream.read_u32_le().await.ok()?;
        self.stat.lock().await.received += cmd_len + 4; // 4 bytes for length prefix

        let mut buffer = vec![0u8; cmd_len as usize];
        stream.read_exact(&mut buffer).await.ok()?;
        let deserialized = Cmd::deserialize(Box::leak(buffer.into_boxed_slice()));
        Some(deserialized)
    }

    fn set_peer_id(&mut self, id: usize) {
        self.receiver_id = Some(id);
    }
    fn get_peer_id(&self) -> Option<usize> {
        self.receiver_id
    }
    async fn get_stat(&self) -> NetStat {
        let stat = self.stat.lock().await;
        stat.clone()
    }
    async fn reset_stat(&mut self) {
        let mut stat = self.stat.lock().await;
        stat.sent = 0;
        stat.received = 0;
    }
}

/// TcpConnectionFactory is responsible for managing TCP connections between peers.
/// It listens for incoming connections and allows connecting to other peers.
/// It maintains a collection of active connections and provides methods to handle communication with peers.
/// Only supports a single peer and creates a single MpcMessageHandler for it.
/// Using connections requires: 
/// - Running listen to allow peers to connect to this peer.
/// - Running connect to connect to peers with lower id
pub struct TcpConnectionFactory {
    id: usize,
    party_num: usize,

    connections: Arc<Mutex<HashMap<usize, ArcPeerConnection>>>,
}

impl TcpConnectionFactory {
    /// Creates a new TcpConnectionFactory with the given ID and party number.
    pub fn new(id: usize, party_num: usize) -> Self {
        let connections = Arc::new(Mutex::new(HashMap::new()));
        TcpConnectionFactory {
            id,
            party_num,
            connections,
        }
    }

    /// Handles a new incoming connection from listen.
    /// Performs a handshake with the peer to establish their peer ID and adds them to the connections map.
    /// 
    /// # Panics
    /// Panics if does not receive a handshake command from the peer.
    pub async fn handle_new_listen_connection(
        connections: Arc<Mutex<HashMap<usize, ArcPeerConnection>>>,
        stream: TcpStream,
    ) {
        stream.set_nodelay(true).expect("Failed to set nodelay on tcp stream (in listen)");
        let mut peer_connection = Box::new(TcpConnection {
            stream: Arc::new(Mutex::new(stream)),
            receiver_id: None,
            stat: NetStat::new_arc(),
        }) as Box<dyn PeerConnection>;

        // Get a handshake command from the peer to know their peer ID
        let handshake_cmd = peer_connection.receive().await;
        if let Cmd::Handshake { peer_id } =
            handshake_cmd.expect("Failed to receive handshake command")
        {
            log::info!("Handshake with peer ID: {}", peer_id);
            peer_connection.set_peer_id(peer_id as usize);

            // connections.insert(peer_id as usize, peer_connection);
            connections
                .lock()
                .await
                .insert(peer_id as usize, Arc::new(Mutex::new(peer_connection)));
        } else {
            panic!("Unexpected command during handshake: ");
        }
    }

    /// Listens for incoming connections on the specified address.
    pub async fn listen(&mut self, listen_addr: SocketAddr) {
        let listener = TcpListener::bind(listen_addr).await.unwrap();
        log::info!("Listening for connections on peer {} with addr {:?}", self.id, listen_addr);

        // Spawn a task to accept incoming connections
        let connections_clone = Arc::clone(&self.connections);
        tokio::spawn(async move {
            while let Ok((stream, receiver_addr)) = listener.accept().await {
                log::info!("received connection from {}", receiver_addr);
                Self::handle_new_listen_connection(
                    connections_clone.clone(),
                    stream,
                )
                .await;
            }
        });
    }

    /// Connects to all peers with lower IDs.
    pub async fn connect(&mut self, peer_infos: &Vec<PeerInfo>) {
        // Connect to peers with lower IDs
        for peer_info in peer_infos {
            if peer_info.id < self.id{
                let addr = SocketAddr::new(
                    peer_info.ip_addr.parse().unwrap(),
                    peer_info.port,
                );
                log::info!(
                    "Connecting {} to peer {} at {}",
                    self.id,
                    peer_info.id,
                    addr,
                );
                let stream = TcpStream::connect(addr).await.unwrap();
                stream.set_nodelay(true).expect("Failed to set nodelay on tcp stream (in connect)");
                let peer_connection = Arc::new(Mutex::new(Box::new(TcpConnection {
                    stream: Arc::new(Mutex::new(stream)),
                    receiver_id: Some(peer_info.id ),
                    stat: NetStat::new_arc(),
                })
                    as Box<dyn PeerConnection>));

                // Send a handshake command to the peer
                // Already know the peer ID since we are connecting to it so no need for receiving a handshake
                let handshake_cmd = Cmd::Handshake { peer_id: self.id as u8 };
                peer_connection.lock().await.send(&handshake_cmd).await;
                self.connections.lock().await.insert(peer_info.id , peer_connection);
            }
        }
    }

    /// Returns the connection to the specified peer.
    pub async fn get(&self, peer_id: usize) -> Option<ArcPeerConnection> {
        self.connections.lock().await.get(&peer_id).cloned()
    }

    /// Returns a MpcMessageHandler that can be used to communicate with all connected peers.
    /// This methods waits until all connections are established and running multiple get_handlers
    /// in the same thread leads to a deadlock.
    /// 
    /// # Expectations
    /// - All connections must be established before calling this method.
    /// - The server must have called `connect` to connect to peers with lower IDs.
    /// - The server must have called `listen` to accept incoming connections and all peers with
    ///   higher IDs must connect to this server.
    pub async fn get_handler(&self) -> MpcMessageHandler {
        // wait till all connections are established
        self.wait_till_ready().await;
        let mut con_handler = MpcMessageHandler::new(self.id as u8);
        // Add all peers
        for (id, con) in self.connections.lock().await.iter() {
            if *id != self.id  {
                con_handler.add_peer(con.clone());
            }
        }
        con_handler
    }

    /// Checks if all connections are established.
    pub async fn is_ready(&self) -> bool{
        return self.connections.lock().await.len() == (self.party_num-1);
    }

    /// Waits until all connections are established.
    pub async fn wait_till_ready(&self) {
        while !self.is_ready().await {
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
    }
}
