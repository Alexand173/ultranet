// ============================================================
// P2P MREŽA ZA ULTRA BLOCKCHAIN 3.0
// ============================================================

use futures::StreamExt;
use libp2p::{
    core::upgrade::Version,
    gossipsub::{self, IdentTopic, MessageAuthenticity},
    identify, identity, kad, mdns, noise, ping,
    swarm::{Config as SwarmConfig, NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    pub ping: ping::Behaviour,
}

#[derive(Debug)]
pub enum UltraEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Kademlia(kad::Event),
    Ping(ping::Event),
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

impl From<ping::Event> for UltraEvent {
    fn from(event: ping::Event) -> Self {
        UltraEvent::Ping(event)
    }
}

// ============================================================
// 3. PEER MANAGER
// ============================================================

pub struct PeerManager {
    pub peers: HashMap<PeerId, PeerInfo>,
    pub banned: Vec<PeerId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PeerAddressSource {
    ConnectionEstablished,
    MdnsDiscovered,
    IdentifyAdvertised,
}

impl PeerAddressSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::ConnectionEstablished => "connection_established",
            Self::MdnsDiscovered => "mdns_discovered",
            Self::IdentifyAdvertised => "identify_advertised",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub address: String,
    pub address_source: PeerAddressSource,
    pub height: u64,
    pub version: u32,
    pub connected_at: u64,
    pub established_connections: u32,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            banned: Vec::new(),
        }
    }

    pub fn add_peer(&mut self, peer_id: PeerId, address: String) {
        self.register_address(peer_id, address, PeerAddressSource::MdnsDiscovered);
    }

    pub fn register_connection(&mut self, peer_id: PeerId, address: String) {
        if self.banned.contains(&peer_id) {
            println!(
                "⚠️ PeerManager registration skipped: source=connection_established peer={peer_id} reason=banned tracked={}",
                self.peers.len()
            );
            return;
        }

        let tracked_peers = self.peers.len();
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            peer.established_connections = peer.established_connections.saturating_add(1);
            if peer.address_source == PeerAddressSource::ConnectionEstablished {
                peer.address = address.clone();
            }
            println!(
                "🧭 PeerManager connection registered: source=connection_established peer={peer_id} address={address} established_connections={} address_source={} tracked={}",
                peer.established_connections,
                peer.address_source.as_str(),
                tracked_peers
            );
            return;
        }

        self.insert_peer(
            peer_id,
            address,
            PeerAddressSource::ConnectionEstablished,
            1,
        );
    }

    pub fn register_address(
        &mut self,
        peer_id: PeerId,
        address: String,
        source: PeerAddressSource,
    ) {
        if self.banned.contains(&peer_id) {
            println!(
                "⚠️ PeerManager registration skipped: source={} peer={peer_id} reason=banned tracked={}",
                source.as_str(),
                self.peers.len()
            );
            return;
        }

        let tracked_peers = self.peers.len();
        if let Some(peer) = self.peers.get_mut(&peer_id) {
            if source >= peer.address_source {
                let previous_source = peer.address_source;
                peer.address = address.clone();
                peer.address_source = source;
                println!(
                    "🔄 PeerManager address promoted: peer={peer_id} from={} to={} address={address} established_connections={} tracked={}",
                    previous_source.as_str(),
                    source.as_str(),
                    peer.established_connections,
                    tracked_peers
                );
            } else {
                println!(
                    "ℹ️ PeerManager address ignored: source={} peer={peer_id} canonical_source={} canonical_address={} tracked={}",
                    source.as_str(),
                    peer.address_source.as_str(),
                    peer.address,
                    tracked_peers
                );
            }
            return;
        }

        self.insert_peer(peer_id, address, source, 0);
    }

    fn insert_peer(
        &mut self,
        peer_id: PeerId,
        address: String,
        address_source: PeerAddressSource,
        established_connections: u32,
    ) {
        self.peers.insert(
            peer_id,
            PeerInfo {
                address: address.clone(),
                address_source,
                height: 0,
                version: 0,
                connected_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                established_connections,
            },
        );
        println!(
            "🧭 PeerManager registered peer: source={} peer={peer_id} address={address} established_connections={established_connections} tracked={}",
            address_source.as_str(),
            self.peers.len()
        );
    }

    pub fn update_height(&mut self, peer_id: &PeerId, height: u64) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.height = height;
            println!("📊 PeerManager status update: peer={peer_id} height={height} tracked=true");
        } else {
            println!(
                "⚠️ PeerManager status for untracked peer: peer={peer_id} height={height} tracked=false"
            );
        }
    }

    pub fn get_best_peers(&self, count: usize) -> Vec<PeerId> {
        let mut peers: Vec<_> = self.peers.iter().collect();
        peers.sort_by(|a, b| b.1.height.cmp(&a.1.height));
        peers.iter().take(count).map(|(id, _)| **id).collect()
    }

    pub fn connection_closed(&mut self, peer_id: &PeerId, remaining_connections: u32) {
        let Some(peer) = self.peers.get_mut(peer_id) else {
            println!(
                "🧹 PeerManager connection close for untracked peer: peer={peer_id} remaining_connections={remaining_connections} tracked={}",
                self.peers.len()
            );
            return;
        };

        peer.established_connections = remaining_connections;
        if remaining_connections > 0 {
            println!(
                "ℹ️ PeerManager retained peer after partial close: peer={peer_id} remaining_connections={remaining_connections} address_source={} tracked={}",
                peer.address_source.as_str(),
                self.peers.len()
            );
            return;
        }

        if peer.address_source == PeerAddressSource::ConnectionEstablished {
            let removed = self.peers.remove(peer_id).is_some();
            println!(
                "🧹 PeerManager removed ephemeral connection record: peer={peer_id} removed={removed} tracked={}",
                self.peers.len()
            );
        } else {
            println!(
                "ℹ️ PeerManager retained discovered peer after final close: peer={peer_id} address_source={} tracked={}",
                peer.address_source.as_str(),
                self.peers.len()
            );
        }
    }

    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        let removed = self.peers.remove(peer_id).is_some();
        println!(
            "🧹 PeerManager removal: peer={peer_id} removed={removed} tracked={}",
            self.peers.len()
        );
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

const PERSISTENT_DIAL_TICK: Duration = Duration::from_secs(1);
const PERSISTENT_DIAL_INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const PERSISTENT_DIAL_MAX_BACKOFF: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
struct PersistentPeerTarget {
    peer_id: PeerId,
    address: Multiaddr,
}

fn parse_persistent_peer_targets(
    raw: &str,
    local_peer_id: &PeerId,
) -> Result<Vec<PersistentPeerTarget>, String> {
    let mut targets = Vec::new();

    for raw_entry in raw.split(',') {
        let entry = raw_entry.trim();
        if entry.is_empty() {
            continue;
        }

        let address = entry
            .parse::<Multiaddr>()
            .map_err(|error| format!("{entry}: invalid multiaddress: {error}"))?;
        let peer_id = address
            .iter()
            .last()
            .and_then(|protocol| match protocol {
                libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
                _ => None,
            })
            .ok_or_else(|| format!("{entry}: address must end with /p2p/<PeerId>"))?;

        if peer_id == *local_peer_id {
            return Err(format!(
                "{entry}: persistent peer cannot be the local PeerId"
            ));
        }
        if targets
            .iter()
            .any(|target: &PersistentPeerTarget| target.peer_id == peer_id)
        {
            return Err(format!("{entry}: duplicate persistent peer {peer_id}"));
        }

        targets.push(PersistentPeerTarget { peer_id, address });
    }

    Ok(targets)
}

fn configured_persistent_peer_targets(local_peer_id: &PeerId) -> Vec<PersistentPeerTarget> {
    match std::env::var("ULTRANET_PERSISTENT_PEERS") {
        Ok(raw) if !raw.trim().is_empty() => {
            match parse_persistent_peer_targets(&raw, local_peer_id) {
                Ok(targets) => {
                    println!(
                        "🔒 Persistent validator dials configured: targets={}",
                        targets.len()
                    );
                    targets
                }
                Err(error) => {
                    eprintln!(
                        "⚠️ Invalid ULTRANET_PERSISTENT_PEERS; persistent dials disabled: {error}"
                    );
                    Vec::new()
                }
            }
        }
        Ok(_) | Err(std::env::VarError::NotPresent) => Vec::new(),
        Err(error) => {
            eprintln!(
                "⚠️ Cannot read ULTRANET_PERSISTENT_PEERS; persistent dials disabled: {error}"
            );
            Vec::new()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistentDialStatus {
    Waiting,
    Dialing,
    Connected,
}

#[derive(Debug, Clone)]
struct PersistentDialState {
    target: PersistentPeerTarget,
    status: PersistentDialStatus,
    attempts: u32,
    next_attempt: Instant,
}

#[derive(Debug, Default)]
struct PersistentDialManager {
    peers: HashMap<PeerId, PersistentDialState>,
}

fn persistent_retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(6);
    let multiplier = 1u64 << exponent;
    let seconds = PERSISTENT_DIAL_INITIAL_BACKOFF
        .as_secs()
        .saturating_mul(multiplier)
        .min(PERSISTENT_DIAL_MAX_BACKOFF.as_secs());
    Duration::from_secs(seconds)
}

impl PersistentDialManager {
    fn new(targets: Vec<PersistentPeerTarget>) -> Self {
        let next_attempt = Instant::now();
        let peers = targets
            .into_iter()
            .map(|target| {
                let peer_id = target.peer_id;
                (
                    peer_id,
                    PersistentDialState {
                        target,
                        status: PersistentDialStatus::Waiting,
                        attempts: 0,
                        next_attempt,
                    },
                )
            })
            .collect();
        Self { peers }
    }

    fn targets(&self) -> Vec<PersistentPeerTarget> {
        self.peers
            .values()
            .map(|state| state.target.clone())
            .collect()
    }

    fn is_configured(&self, peer_id: &PeerId) -> bool {
        self.peers.contains_key(peer_id)
    }

    fn take_due_dials(&mut self, now: Instant) -> Vec<(PersistentPeerTarget, u32)> {
        let mut due = Vec::new();
        for state in self.peers.values_mut() {
            if state.status == PersistentDialStatus::Waiting && now >= state.next_attempt {
                state.status = PersistentDialStatus::Dialing;
                due.push((state.target.clone(), state.attempts.saturating_add(1)));
            }
        }
        due
    }

    fn mark_connected(&mut self, peer_id: &PeerId) {
        if let Some(state) = self.peers.get_mut(peer_id) {
            state.status = PersistentDialStatus::Connected;
            state.attempts = 0;
            state.next_attempt = Instant::now();
        }
    }

    fn schedule_retry(&mut self, peer_id: &PeerId) -> Option<Duration> {
        let state = self.peers.get_mut(peer_id)?;
        state.status = PersistentDialStatus::Waiting;
        state.attempts = state.attempts.saturating_add(1);
        let delay = persistent_retry_delay(state.attempts);
        state.next_attempt = Instant::now() + delay;
        Some(delay)
    }

    fn counts(&self) -> (usize, usize, usize) {
        self.peers.values().fold((0, 0, 0), |mut counts, state| {
            match state.status {
                PersistentDialStatus::Waiting => counts.0 += 1,
                PersistentDialStatus::Dialing => counts.1 += 1,
                PersistentDialStatus::Connected => counts.2 += 1,
            }
            counts
        })
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
    persistent_dials: PersistentDialManager,
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

fn describe_disconnect_cause<E: std::fmt::Debug>(
    cause: Option<&libp2p::swarm::ConnectionError<E>>,
) -> (&'static str, String) {
    match cause {
        None => (
            "clean",
            "active close completed without an error".to_string(),
        ),
        Some(libp2p::swarm::ConnectionError::IO(error)) => ("io", error.to_string()),
        Some(libp2p::swarm::ConnectionError::KeepAliveTimeout) => (
            "keep_alive_timeout",
            "connection keep-alive timeout expired".to_string(),
        ),
        #[allow(deprecated)]
        Some(libp2p::swarm::ConnectionError::Handler(error)) => ("handler", format!("{error:?}")),
    }
}

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

        // 6. Ping keep-alive
        let ping = ping::Behaviour::new(
            ping::Config::new()
                .with_interval(Duration::from_secs(15))
                .with_timeout(Duration::from_secs(10)),
        );

        // 7. Kademlia DHT
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
            ping,
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
        let persistent_dials =
            PersistentDialManager::new(configured_persistent_peer_targets(&peer_id));

        Ok(Self {
            swarm,
            peer_id,
            blockchain,
            peer_manager: Arc::new(Mutex::new(PeerManager::new())),
            persistent_dials,
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

    fn drive_persistent_dials(&mut self) {
        let due_dials = self.persistent_dials.take_due_dials(Instant::now());

        for (target, attempt) in due_dials {
            match self.swarm.dial(target.address.clone()) {
                Ok(()) => println!(
                    "🔒 Persistent dial started: peer={} address={} attempt={attempt}",
                    target.peer_id, target.address
                ),
                Err(error) => {
                    let retry_delay = self.persistent_dials.schedule_retry(&target.peer_id);
                    println!(
                        "⚠️ Persistent dial rejected: peer={} address={} attempt={attempt} error={error:?} retry_in_seconds={}",
                        target.peer_id,
                        target.address,
                        retry_delay.map_or(0, |delay| delay.as_secs())
                    );
                }
            }
        }
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
            SwarmEvent::Behaviour(UltraEvent::Ping(event)) => {
                if self.persistent_dials.is_configured(&event.peer) {
                    if let Err(error) = event.result {
                        eprintln!(
                            "⚠️ Persistent peer ping failed: peer={} connection={:?} error={error}",
                            event.peer, event.connection
                        );
                    }
                }
            }
            SwarmEvent::Behaviour(UltraEvent::Mdns(mdns::Event::Discovered(list))) => {
                println!("🔍 mDNS discovery event: candidates={}", list.len());
                for (peer_id, addr) in list {
                    println!("🔍 mDNS discovered peer: {} at {}", peer_id, addr);
                    self.peer_manager.lock().register_address(
                        peer_id,
                        addr.to_string(),
                        PeerAddressSource::MdnsDiscovered,
                    );
                }
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint,
                num_established,
                ..
            } => {
                let remote_address = endpoint.get_remote_address().clone();
                println!(
                    "✅ libp2p connection established: peer={peer_id} remote_address={remote_address} endpoint={endpoint:?} simultaneous_connections={num_established}"
                );
                if self.persistent_dials.is_configured(&peer_id) {
                    self.persistent_dials.mark_connected(&peer_id);
                    println!("🔒 Persistent peer connected: peer={peer_id}");
                }
                self.peer_manager
                    .lock()
                    .register_connection(peer_id, remote_address.to_string());
                self.send_status().await;
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                cause,
            } => {
                let (reason_kind, reason_detail) = describe_disconnect_cause(cause.as_ref());
                println!(
                    "❌ libp2p connection closed: peer={peer_id} connection_id={connection_id:?} endpoint_role={} remote_address={} remaining_connections={num_established} reason_kind={reason_kind} reason_detail={reason_detail:?} cause={cause:?}",
                    if endpoint.is_dialer() {
                        "dialer"
                    } else {
                        "listener"
                    },
                    endpoint.get_remote_address()
                );
                self.peer_manager
                    .lock()
                    .connection_closed(&peer_id, num_established);
                if num_established == 0 {
                    if let Some(retry_delay) = self.persistent_dials.schedule_retry(&peer_id) {
                        println!(
                            "🔁 Persistent peer retry scheduled: peer={peer_id} delay_seconds={} reason_kind={reason_kind}",
                            retry_delay.as_secs()
                        );
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } => {
                if self.persistent_dials.is_configured(&peer_id) {
                    let retry_delay = self.persistent_dials.schedule_retry(&peer_id);
                    println!(
                        "⚠️ Persistent dial failed: peer={peer_id} error={error:?} retry_in_seconds={}",
                        retry_delay.map_or(0, |delay| delay.as_secs())
                    );
                }
            }
            SwarmEvent::Behaviour(UltraEvent::Identify(identify::Event::Received {
                peer_id,
                info,
            })) => {
                println!(
                    "🆔 libp2p identify received: peer={peer_id} protocol={} listen_addresses={}",
                    info.protocol_version,
                    info.listen_addrs.len()
                );
                // Identify advertises addresses suitable for future dials, so it
                // promotes the connection-observed address to the canonical record.
                for addr in info.listen_addrs {
                    println!("🧭 Kademlia learned peer address: peer={peer_id} address={addr}");
                    self.peer_manager.lock().register_address(
                        peer_id,
                        addr.to_string(),
                        PeerAddressSource::IdentifyAdvertised,
                    );
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
        for target in self.persistent_dials.targets() {
            self.swarm
                .behaviour_mut()
                .kademlia
                .add_address(&target.peer_id, target.address.clone());
            println!(
                "🔒 Added persistent validator target: peer={} address={}",
                target.peer_id, target.address
            );
        }
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

        let mut sync_timer = tokio::time::interval(Duration::from_secs(30));
        let mut discovery_timer = tokio::time::interval(Duration::from_secs(60));
        let mut persistent_dial_timer = tokio::time::interval(PERSISTENT_DIAL_TICK);

        while self.running {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_event(event).await;
                }
                _ = persistent_dial_timer.tick() => {
                    self.drive_persistent_dials();
                }
                _ = sync_timer.tick() => {
                    self.sync_chain().await;
                    let tracked_peers = self.peer_manager.lock().peers.len();
                    let connected_peers = self.swarm.connected_peers().count();
                    let (persistent_waiting, persistent_dialing, persistent_connected) =
                        self.persistent_dials.counts();
                    println!(
                        "⏰ Heartbeat - PeerManager tracked peers: {tracked_peers}; libp2p connected peers: {connected_peers}; persistent waiting={persistent_waiting} dialing={persistent_dialing} connected={persistent_connected}"
                    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_registration_waits_for_final_close() {
        let peer_id = PeerId::random();
        let mut manager = PeerManager::new();

        manager.register_connection(peer_id, "/ip4/203.0.113.10/tcp/9000".into());
        manager.register_connection(peer_id, "/ip4/203.0.113.10/tcp/9001".into());

        let peer = manager.peers.get(&peer_id).expect("peer should be tracked");
        assert_eq!(peer.established_connections, 2);
        assert_eq!(
            peer.address_source,
            PeerAddressSource::ConnectionEstablished
        );

        manager.connection_closed(&peer_id, 1);
        assert_eq!(
            manager
                .peers
                .get(&peer_id)
                .expect("peer should remain during a second connection")
                .established_connections,
            1
        );

        manager.connection_closed(&peer_id, 0);
        assert!(!manager.peers.contains_key(&peer_id));
    }

    #[test]
    fn identify_promotes_address_and_mdns_cannot_downgrade_it() {
        let peer_id = PeerId::random();
        let mut manager = PeerManager::new();

        manager.register_connection(peer_id, "/ip4/203.0.113.10/tcp/9000".into());
        manager.register_address(
            peer_id,
            "/ip4/198.51.100.20/tcp/9000/p2p/peer".into(),
            PeerAddressSource::IdentifyAdvertised,
        );
        manager.register_address(
            peer_id,
            "/ip4/192.0.2.5/tcp/9000/p2p/peer".into(),
            PeerAddressSource::MdnsDiscovered,
        );

        let peer = manager.peers.get(&peer_id).expect("peer should be tracked");
        assert_eq!(peer.address, "/ip4/198.51.100.20/tcp/9000/p2p/peer");
        assert_eq!(peer.address_source, PeerAddressSource::IdentifyAdvertised);
        assert_eq!(peer.established_connections, 1);

        manager.connection_closed(&peer_id, 0);
        assert_eq!(
            manager
                .peers
                .get(&peer_id)
                .expect("identified peers remain as known records after disconnect")
                .established_connections,
            0
        );
    }

    #[test]
    fn persistent_peer_parser_requires_full_unique_non_local_addresses() {
        let local_peer_id = PeerId::random();
        let remote_peer_id = PeerId::random();
        let address = format!("/ip4/203.0.113.10/tcp/9000/p2p/{remote_peer_id}");

        let targets = parse_persistent_peer_targets(&format!(" , {address} "), &local_peer_id)
            .expect("valid persistent peer should parse");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].peer_id, remote_peer_id);
        assert_eq!(targets[0].address.to_string(), address);

        let duplicate_error =
            parse_persistent_peer_targets(&format!("{address},{address}"), &local_peer_id)
                .expect_err("duplicate peer IDs should be rejected");
        assert!(duplicate_error.contains("duplicate persistent peer"));

        let missing_peer_error =
            parse_persistent_peer_targets("/ip4/203.0.113.10/tcp/9000", &local_peer_id)
                .expect_err("persistent targets must include a peer ID");
        assert!(missing_peer_error.contains("must end with /p2p/<PeerId>"));

        let self_error = parse_persistent_peer_targets(
            &format!("/ip4/203.0.113.10/tcp/9000/p2p/{local_peer_id}"),
            &local_peer_id,
        )
        .expect_err("self-dials should be rejected");
        assert!(self_error.contains("local PeerId"));
    }

    #[test]
    fn persistent_dial_backoff_is_bounded_and_resets_after_connection() {
        assert_eq!(persistent_retry_delay(1), Duration::from_secs(5));
        assert_eq!(persistent_retry_delay(2), Duration::from_secs(10));
        assert_eq!(persistent_retry_delay(7), Duration::from_secs(300));
        assert_eq!(persistent_retry_delay(100), Duration::from_secs(300));

        let peer_id = PeerId::random();
        let target = PersistentPeerTarget {
            peer_id,
            address: format!("/ip4/203.0.113.10/tcp/9000/p2p/{peer_id}")
                .parse()
                .unwrap(),
        };
        let mut manager = PersistentDialManager::new(vec![target]);

        assert_eq!(manager.take_due_dials(Instant::now()).len(), 1);
        assert!(manager.take_due_dials(Instant::now()).is_empty());
        assert_eq!(
            manager.schedule_retry(&peer_id),
            Some(Duration::from_secs(5))
        );
        assert_eq!(manager.counts(), (1, 0, 0));

        manager.mark_connected(&peer_id);
        assert_eq!(manager.counts(), (0, 0, 1));
        assert_eq!(
            manager.schedule_retry(&peer_id),
            Some(Duration::from_secs(5))
        );
        assert_eq!(manager.counts(), (1, 0, 0));
        assert!(manager.schedule_retry(&PeerId::random()).is_none());
    }
}
