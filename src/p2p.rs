// ============================================================
// P2P MREŽA ZA ULTRA BLOCKCHAIN 3.0
// ============================================================

use futures::StreamExt;
use libp2p::{
    core::upgrade::Version,
    gossipsub::{self, IdentTopic, MessageAuthenticity},
    identify, identity, kad, mdns, noise,
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock as TokioRwLock;

use crate::{Transaction, UltraBlock, UltraBlockchain};

// ============================================================
// 1. MREŽNE PORUKE
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMessage {
    pub tx: Transaction,
    pub zk_proof: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMessage {
    pub block: UltraBlock,
    pub zk_proof: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Transaction(TransactionMessage),
    Block(BlockMessage),
    GetBlocks { from: u64, to: u64 },
    Blocks { blocks: Vec<UltraBlock> },
    Status { version: u32, height: u64 },
    Ping,
    Pong,
}

// ============================================================
// 2. ULTRA BEHAVIOR
// ============================================================

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "UltraEvent")]
pub struct UltraBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
}

#[derive(Debug)]
pub enum UltraEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Kademlia(kad::Event),
}

impl From<gossipsub::Event> for UltraEvent {
    fn from(event: gossipsub::Event) -> Self {
        UltraEvent::Gossipsub(event)
    }
}

impl From<mdns::Event> for UltraEvent {
    fn from(event: mdns::Event) -> Self {
        UltraEvent::Mdns(event)
    }
}

impl From<identify::Event> for UltraEvent {
    fn from(event: identify::Event) -> Self {
        UltraEvent::Identify(event)
    }
}

impl From<kad::Event> for UltraEvent {
    fn from(event: kad::Event) -> Self {
        UltraEvent::Kademlia(event)
    }
}

// ============================================================
// 3. PEER MANAGER
// ============================================================

pub struct PeerManager {
    pub peers: HashMap<PeerId, PeerInfo>,
    pub banned: Vec<PeerId>,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub address: String,
    pub height: u64,
    pub version: u32,
    pub connected_at: u64,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            banned: Vec::new(),
        }
    }

    pub fn add_peer(&mut self, peer_id: PeerId, address: String) {
        if !self.banned.contains(&peer_id) {
            self.peers.entry(peer_id).or_insert(PeerInfo {
                address,
                height: 0,
                version: 0,
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            });
        }
    }

    pub fn update_height(&mut self, peer_id: &PeerId, height: u64) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.height = height;
        }
    }

    pub fn get_best_peers(&self, count: usize) -> Vec<PeerId> {
        let mut peers: Vec<_> = self.peers.iter().collect();
        peers.sort_by(|a, b| b.1.height.cmp(&a.1.height));
        peers.iter().take(count).map(|(id, _)| **id).collect()
    }

    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    pub fn ban_peer(&mut self, peer_id: PeerId) {
        self.banned.push(peer_id);
        self.peers.remove(&peer_id);
        println!("⚠️ Peer banned: {}", peer_id);
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 4. P2P NODE
// ============================================================

pub struct P2PNode {
    pub swarm: Swarm<UltraBehaviour>,
    pub peer_id: PeerId,
    pub blockchain: Arc<TokioRwLock<UltraBlockchain>>,
    pub peer_manager: Arc<Mutex<PeerManager>>,
    pub running: bool,
}

/// Public UltraNet bootstrap nodes.
///
/// This address is the live VPS node currently advertised on port 9000.
/// The node identity is regenerated on every process start, so this address
/// must be updated whenever the bootstrap node is restarted until identity
/// persistence is implemented.
pub const BOOTNODES: &[&str] =
    &["/ip4/167.233.161.115/tcp/9000/p2p/12D3KooWRFWD4VDW7g2t4VEmajjyfrGh5ZuQUoPVxFeq7ffRetgP"];

impl P2PNode {
    pub async fn new(
        blockchain: Arc<TokioRwLock<UltraBlockchain>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // 1. Generiši identitet
        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = local_key.public().to_peer_id();

        // 2. Transport (TCP + Noise autentikacija + Yamux multipleksiranje)
        let transport = tcp::tokio::Transport::default()
            .upgrade(Version::V1)
            .authenticate(noise::Config::new(&local_key)?)
            .multiplex(yamux::Config::default())
            .boxed();

        // 3. Gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()?;

        // NAPOMENA: `ValidationMode::Strict` zahteva da poruke budu POTPISANE
        // (ne samo da imaju autora), inače `gossipsub::Behaviour::new` vraća
        // grešku konfiguracije. Zato koristimo `MessageAuthenticity::Signed`.
        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )?;

        // 4. mDNS
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

        // 5. Identify
        let identify = identify::Behaviour::new(identify::Config::new(
            "ultra/1.0.0".to_string(),
            local_key.public(),
        ));

        // 6. Kademlia DHT
        let store = kad::store::MemoryStore::new(peer_id);
        let mut kad_config = kad::Config::default();
        kad_config.set_query_timeout(Duration::from_secs(5 * 60));
        let kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);

        // 7. Behaviour
        let behaviour = UltraBehaviour {
            gossipsub,
            mdns,
            identify,
            kademlia,
        };

        // 7. Swarm
        let mut swarm = Swarm::new(
            transport,
            behaviour,
            peer_id,
            SwarmConfig::with_tokio_executor(),
        );

        // 8. Subscribe na topic
        let topic = IdentTopic::new("ultra-net");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        Ok(Self {
            swarm,
            peer_id,
            blockchain,
            peer_manager: Arc::new(Mutex::new(PeerManager::new())),
            running: true,
        })
    }

    pub fn start_listening(&mut self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let addr: Multiaddr = addr.parse()?;
        self.swarm.listen_on(addr.clone())?;
        println!("📡 P2P listening on: {}", addr);
        Ok(())
    }

    pub fn dial(&mut self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let addr: Multiaddr = addr.parse()?;
        self.swarm.dial(addr.clone())?;
        println!("🔗 Dialing: {}", addr);
        Ok(())
    }

    pub fn add_bootnode(&mut self, peer_id: PeerId, addr: Multiaddr) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, addr);
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
    }

    pub async fn broadcast_transaction(
        &mut self,
        tx: Transaction,
        zk_proof: Vec<u8>,
    ) -> Result<(), String> {
        let msg = NetworkMessage::Transaction(TransactionMessage { tx, zk_proof });
        let data = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
        let topic = IdentTopic::new("ultra-net");
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map_err(|e| e.to_string())?;
        println!("📤 Broadcasted transaction");
        Ok(())
    }

    pub async fn broadcast_block(
        &mut self,
        block: UltraBlock,
        zk_proof: Vec<u8>,
    ) -> Result<(), String> {
        let msg = NetworkMessage::Block(BlockMessage { block, zk_proof });
        let data = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
        let topic = IdentTopic::new("ultra-net");
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map_err(|e| e.to_string())?;
        println!("📤 Broadcasted block");
        Ok(())
    }

    // NAPOMENA: Ne koristimo eksplicitni tip `SwarmEvent<UltraEvent>` ovde -
    // u libp2p 0.52 `SwarmEvent` ima DVA generic parametra
    // (`TBehaviourOutEvent`, `THandlerErr`), a `THandlerErr` tip je dugačak i
    // ne-trivijalan za ručno pisanje. Umesto toga, oslanjamo se na inferenciju
    // tipova - ova funkcija je generic i kompajler izvodi konkretan tip iz
    // pozivnog mesta (`run()`), isto kao u zvaničnim libp2p primerima.
    async fn handle_event<E: std::fmt::Debug>(&mut self, event: SwarmEvent<UltraEvent, E>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("📡 Listening on: {}", address);
            }
            SwarmEvent::Behaviour(UltraEvent::Gossipsub(gossipsub::Event::Message {
                message,
                ..
            })) => {
                if let Ok(msg) = serde_json::from_slice::<NetworkMessage>(&message.data) {
                    self.handle_message(msg, message.source).await;
                }
            }
            SwarmEvent::Behaviour(UltraEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer_id, addr) in list {
                    println!("🔍 Discovered peer: {} at {}", peer_id, addr);
                    self.peer_manager.lock().add_peer(peer_id, addr.to_string());
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("✅ Connected to: {}", peer_id);
                self.send_status().await;
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("❌ Disconnected from: {}", peer_id);
                self.peer_manager.lock().remove_peer(&peer_id);
            }
            SwarmEvent::Behaviour(UltraEvent::Identify(identify::Event::Received {
                peer_id,
                info,
            })) => {
                println!(
                    "🆔 Peer identified: {} (version: {})",
                    peer_id, info.protocol_version
                );
                // Dodaj u Kademlia routing tabelu čim identifikujemo peer-a
                for addr in info.listen_addrs {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                }
            }
            SwarmEvent::Behaviour(UltraEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                result,
                ..
            })) => match result {
                kad::QueryResult::Bootstrap(Ok(res)) => {
                    println!(
                        "🌐 Kademlia Bootstrap success: {} remains",
                        res.num_remaining
                    );
                }
                kad::QueryResult::Bootstrap(Err(e)) => {
                    println!("⚠️ Kademlia Bootstrap error: {:?}", e);
                }
                _ => {}
            },
            _ => {}
        }
    }

    async fn handle_message(&mut self, msg: NetworkMessage, source: Option<PeerId>) {
        match msg {
            NetworkMessage::Transaction(tx_msg) => {
                println!(
                    "📝 Received transaction: {} -> {}",
                    tx_msg.tx.sender, tx_msg.tx.recipient
                );
                let blockchain = self.blockchain.write().await;
                let _ = blockchain.add_transaction(tx_msg.tx);
            }
            NetworkMessage::Block(block_msg) => {
                println!("⛓️ Received block: {}", block_msg.block.index);
                let mut blockchain = self.blockchain.write().await;
                if let Err(e) = blockchain.add_remote_block(block_msg.block, block_msg.zk_proof) {
                    eprintln!("❌ Failed to add remote block: {}", e);
                }
            }
            NetworkMessage::Status { version, height } => {
                println!("📊 Peer status: version={}, height={}", version, height);
                let peer_id = source.unwrap_or_else(PeerId::random);
                self.peer_manager.lock().update_height(&peer_id, height);
            }
            NetworkMessage::GetBlocks { from, to } => {
                println!("📥 Peer requesting blocks {} to {}", from, to);
            }
            NetworkMessage::Blocks { blocks } => {
                println!("📦 Received {} blocks", blocks.len());
                let mut blockchain = self.blockchain.write().await;
                for block in blocks {
                    // Kod sync-a (bulk load), rekurzivni dokazi se obično generišu ili
                    // preuzimaju u odvojenom stream-u. Za sada šaljemo prazan proof.
                    if let Err(e) = blockchain.add_remote_block(block, vec![]) {
                        eprintln!("❌ Failed to add synced block: {}", e);
                        break;
                    }
                }
                drop(blockchain);
                // Proaktivno nastavi sync ako ima još blokova
                self.sync_chain().await;
            }
            _ => {}
        }
    }

    async fn send_status(&mut self) {
        let blockchain = self.blockchain.read().await;
        let status = NetworkMessage::Status {
            version: blockchain.version,
            height: blockchain.chain.len() as u64,
        };
        drop(blockchain);
        let data = match serde_json::to_vec(&status) {
            Ok(d) => d,
            Err(_) => return,
        };
        let topic = IdentTopic::new("ultra-net");
        let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, data);
    }

    async fn sync_chain(&mut self) {
        let blockchain = self.blockchain.read().await;
        let our_height = blockchain.chain.len() as u64;
        drop(blockchain);
        let peers = self.peer_manager.lock().get_best_peers(3);

        for peer_id in peers {
            let target_height = {
                let peer_manager = self.peer_manager.lock();
                peer_manager.peers.get(&peer_id).map(|peer| peer.height)
            };

            if let Some(height) = target_height {
                if height > our_height {
                    println!(
                        "🔄 Syncing with peer: {} (their height: {})",
                        peer_id, height
                    );
                    let msg = NetworkMessage::GetBlocks {
                        from: our_height + 1,
                        to: height,
                    };
                    let data = match serde_json::to_vec(&msg) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let topic = IdentTopic::new("ultra-net");
                    let _ = self.swarm.behaviour_mut().gossipsub.publish(topic, data);
                    break;
                }
            }
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 P2P Node running!");
        println!("🔑 Peer ID: {}", self.peer_id);
        println!("📋 Network: ultra-net");
        println!();

        // 1. Kademlia Bootstrap
        for addr_str in BOOTNODES {
            if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                if let Some(peer_id) = addr.iter().last().and_then(|p| {
                    if let libp2p::multiaddr::Protocol::P2p(peer_id) = p {
                        Some(peer_id)
                    } else {
                        None
                    }
                }) {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr.clone());
                    println!("🚀 Added Bootnode: {}", peer_id);
                }
            }
        }
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

        let mut sync_timer = tokio::time::interval(Duration::from_secs(30));
        let mut discovery_timer = tokio::time::interval(Duration::from_secs(60));

        while self.running {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_event(event).await;
                }
                _ = sync_timer.tick() => {
                    self.sync_chain().await;
                    let count = self.peer_manager.lock().peers.len();
                    println!("⏰ Heartbeat - Active peers: {}", count);
                }
                _ = discovery_timer.tick() => {
                    println!("🔍 P2P Discovery: Running iterative query...");
                    // Generiši nasumični PeerId za pretragu kako bismo otkrili nove peer-ove u DHT-u
                    let random_peer = PeerId::random();
                    self.swarm.behaviour_mut().kademlia.get_closest_peers(random_peer);
                }
            }
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
        println!("🛑 P2P Node stopping...");
    }
}
