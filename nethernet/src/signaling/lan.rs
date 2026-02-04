use crate::error::{NethernetError, Result};
use crate::protocol::Signal;
use crate::signaling::Signaling;
use crate::discovery::{self, RequestPacket, ResponsePacket, MessagePacket, ServerData};
use futures::Stream;
use std::collections::HashMap;
use std::net::{SocketAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock, Mutex};
use std::time::{Duration, Instant};

const BROADCAST_INTERVAL: Duration = Duration::from_secs(2);
const ADDRESS_TIMEOUT: Duration = Duration::from_secs(15);

pub struct LanSignaling {
    network_id: u64,
    socket: Arc<UdpSocket>,
    addresses: Arc<RwLock<HashMap<u64, AddressEntry>>>,
    signal_tx: mpsc::UnboundedSender<Signal>,
    signal_rx: Arc<Mutex<mpsc::UnboundedReceiver<Signal>>>,
    server_data: Arc<RwLock<Option<ServerData>>>,
    discovered_servers: Arc<RwLock<HashMap<u64, ServerData>>>,
    broadcast_addr: Option<SocketAddr>,
}

struct AddressEntry {
    addr: SocketAddr,
    last_seen: Instant,
}

impl LanSignaling {
    pub async fn new(network_id: u64, bind_addr: SocketAddr) -> Result<Self> {
        Self::new_with_reuse(network_id, bind_addr, false).await
    }
    
    pub async fn new_with_reuse(network_id: u64, bind_addr: SocketAddr, reuse_port: bool) -> Result<Self> {
        let socket = if reuse_port {
            // Use socket2 for SO_REUSEPORT support
            let socket = socket2::Socket::new(
                socket2::Domain::for_address(bind_addr),
                socket2::Type::DGRAM,
                Some(socket2::Protocol::UDP),
            )?;
            
            socket.set_reuse_address(true)?;
            #[cfg(unix)]
            socket.set_reuse_port(true)?;
            socket.set_nonblocking(true)?;
            socket.bind(&bind_addr.into())?;
            
            // Convert to tokio UdpSocket
            UdpSocket::from_std(socket.into())?
        } else {
            UdpSocket::bind(bind_addr).await?
        };
        
        socket.set_broadcast(true)?;
        
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        
        // If not binding to 7551, enable broadcast to 7551
        let broadcast_addr = if bind_addr.port() != 7551 {
            Some(SocketAddr::new(Ipv4Addr::BROADCAST.into(), 7551))
        } else {
            None
        };
        
        let signaling = Self {
            network_id,
            socket: Arc::new(socket),
            addresses: Arc::new(RwLock::new(HashMap::new())),
            signal_tx,
            signal_rx: Arc::new(Mutex::new(signal_rx)),
            server_data: Arc::new(RwLock::new(None)),
            discovered_servers: Arc::new(RwLock::new(HashMap::new())),
            broadcast_addr,
        };

        signaling.start_background_tasks();
        Ok(signaling)
    }
    
    pub async fn discover(&self) -> Result<HashMap<u64, ServerData>> {
        Ok(self.discovered_servers.read().await.clone())
    }

    fn start_background_tasks(&self) {
        let socket = self.socket.clone();
        let addresses = self.addresses.clone();
        let signal_tx = self.signal_tx.clone();
        let server_data = self.server_data.clone();
        let discovered_servers = self.discovered_servers.clone();
        let network_id = self.network_id;

        // Packet receiver task
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((n, addr)) => {
                        let _ = Self::handle_packet(
                            &buf[..n],
                            addr,
                            network_id,
                            &addresses,
                            &signal_tx,
                            &socket,
                            &server_data,
                            &discovered_servers,
                        ).await;
                    }
                    Err(e) => {
                        tracing::debug!("Socket receive error: {}", e);
                        break;
                    }
                }
            }
        });

        // Cleanup and broadcast task
        let addresses_cleanup = self.addresses.clone();
        let socket_broadcast = self.socket.clone();
        let broadcast_addr = self.broadcast_addr;
        let network_id_broadcast = self.network_id;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(BROADCAST_INTERVAL);
            loop {
                interval.tick().await;
                Self::cleanup_addresses(&addresses_cleanup).await;
                
                // Send broadcast request if client
                if let Some(addr) = broadcast_addr {
                    let _ = Self::send_request(&socket_broadcast, network_id_broadcast, addr).await;
                }
            }
        });
    }
    
    async fn send_request(socket: &UdpSocket, network_id: u64, addr: SocketAddr) -> Result<()> {
        let request = RequestPacket;
        let data = discovery::marshal(&request, network_id)?;
        socket.send_to(&data, addr).await?;
        Ok(())
    }

    async fn handle_packet(
        data: &[u8],
        addr: SocketAddr,
        own_network_id: u64,
        addresses: &Arc<RwLock<HashMap<u64, AddressEntry>>>,
        signal_tx: &mpsc::UnboundedSender<Signal>,
        socket: &Arc<UdpSocket>,
        server_data: &Arc<RwLock<Option<ServerData>>>,
        discovered_servers: &Arc<RwLock<HashMap<u64, ServerData>>>,
    ) -> Result<()> {
        // Unmarshal the encrypted packet
        let (packet, sender_id) = match discovery::unmarshal(data) {
            Ok(result) => result,
            Err(_) => return Ok(()),
        };
        
        if sender_id == own_network_id {
            return Ok(());
        }

        // Update address mapping
        {
            let mut addrs = addresses.write().await;
            addrs.insert(sender_id, AddressEntry {
                addr,
                last_seen: Instant::now(),
            });
        }

        match packet.id() {
            discovery::ID_REQUEST_PACKET => {
                // Request packet - send response if we have server data
                if let Some(data) = server_data.read().await.as_ref() {
                    let app_data = data.marshal()?;
                    let response = ResponsePacket::new(app_data);
                    let response_data = discovery::marshal(&response, own_network_id)?;
                    let _ = socket.send_to(&response_data, addr).await;
                }
            }
            discovery::ID_RESPONSE_PACKET => {
                // Response packet - parse and store server data
                let response = packet.as_any()
                    .downcast_ref::<ResponsePacket>()
                    .ok_or_else(|| NethernetError::Other("failed to downcast ResponsePacket".to_string()))?;
                
                if let Ok(server_info) = ServerData::unmarshal(&response.application_data) {
                    discovered_servers.write().await.insert(sender_id, server_info);
                }
            }
            discovery::ID_MESSAGE_PACKET => {
                // Message packet - parse signaling data
                let message = packet.as_any()
                    .downcast_ref::<MessagePacket>()
                    .ok_or_else(|| NethernetError::Other("failed to downcast MessagePacket".to_string()))?;
                
                // Ignore Ping messages - these are not WebRTC negotiation signals
                let trimmed_data = message.data.trim();
                if trimmed_data == "Ping" || trimmed_data.starts_with("Ping ") {
                    return Ok(());
                }
                
                // Only process WebRTC signals if message is for us
                if message.recipient_id == own_network_id {
                    if let Ok(signal) = Signal::from_string(&message.data, sender_id.to_string()) {
                        let _ = signal_tx.send(signal);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn cleanup_addresses(addresses: &Arc<RwLock<HashMap<u64, AddressEntry>>>) {
        let mut addrs = addresses.write().await;
        addrs.retain(|_, entry| entry.last_seen.elapsed() < ADDRESS_TIMEOUT);
    }
}

impl Signaling for LanSignaling {
    async fn signal(&self, signal: Signal) -> Result<()> {
        let network_id = signal.network_id.parse::<u64>()
            .map_err(|e| NethernetError::Other(format!("Invalid network ID: {}", e)))?;

        let addr = {
            let addrs = self.addresses.read().await;
            addrs.get(&network_id)
                .map(|entry| entry.addr)
                .ok_or_else(|| NethernetError::Other(format!("Address not found for network {}", network_id)))?
        };

        let signal_str = signal.to_string();
        let message = MessagePacket::new(network_id, signal_str);
        let data = discovery::marshal(&message, self.network_id)?;

        self.socket.send_to(&data, addr).await?;
        Ok(())
    }

    fn signals(&self) -> Pin<Box<dyn Stream<Item = Signal> + Send>> {
        let rx = self.signal_rx.clone();
        Box::pin(futures::stream::unfold(rx, |rx| async move {
            let mut locked_rx = rx.lock().await;
            let signal = locked_rx.recv().await;
            drop(locked_rx);
            signal.map(|s| (s, rx))
        }))
    }

    fn network_id(&self) -> String {
        self.network_id.to_string()
    }

    fn set_pong_data(&self, data: Vec<u8>) {
        let server_data = self.server_data.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                *server_data.write().await = ServerData::unmarshal(&data).ok();
            });
        });
    }
}
