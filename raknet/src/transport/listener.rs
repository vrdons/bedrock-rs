mod offline;
mod online;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::protocol::constants::{self, UDP_HEADER_SIZE};
use crate::transport::listener_conn::SessionState;
use crate::transport::mux::new_tick_interval;
use crate::transport::stream::RaknetStream;

use offline::PendingConnection;

use online::{dispatch_datagram, handle_outgoing_msg, tick_sessions};

/// Configuration for a [`RaknetListener`].
#[derive(Debug, Clone)]
pub struct RaknetListenerConfig {
    /// Address to start server.
    pub bind_addr: Option<SocketAddr>,

    /// Maximum number of concurrent connections allowed.
    pub max_connections: usize,

    /// Maximum number of pending connections (handshakes).
    pub max_pending_connections: usize,

    /// Maximum MTU size to support/advertise.
    pub max_mtu: u16,

    /// Optional socket receive buffer size.
    pub socket_recv_buffer_size: Option<usize>,

    /// Optional socket send buffer size.
    pub socket_send_buffer_size: Option<usize>,

    /// Timeout duration for inactive sessions.
    pub session_timeout: Duration,

    /// Duration before a session is considered stale.
    pub session_stale: Duration,

    /// Maximum bytes of reliable data to queue for a single session before disconnecting.
    pub max_queued_reliable_bytes: usize,

    /// Initial advertisement string.
    pub advertisement: Vec<u8>,

    /// Maximum number of ordering channels.
    pub max_ordering_channels: usize,

    /// Maximum capacity of the ACK queue.
    pub ack_queue_capacity: usize,

    /// Timeout for reassembling split packets.
    pub split_timeout: Duration,

    /// Maximum window size for reliable packets.
    pub reliable_window: u32,

    /// Maximum number of parts in a single split packet.
    pub max_split_parts: u32,

    /// Maximum number of concurrent split packets being reassembled.
    pub max_concurrent_splits: usize,
}

impl Default for RaknetListenerConfig {
    fn default() -> Self {
        Self {
            bind_addr: None,
            max_connections: 1024,
            max_pending_connections: 1024,
            max_mtu: 1400,
            socket_recv_buffer_size: None,
            socket_send_buffer_size: None,
            session_timeout: Duration::from_secs(10),
            session_stale: Duration::from_secs(5),
            max_queued_reliable_bytes: 4 * 1024 * 1024, // 4MB
            advertisement: b"MCPE;Tokio-Raknet Default Advertisement;527;1.19.1;0;10;13253860892328930865;Tokio Raknet;Survival;1;19132;19133".to_vec(),
            max_ordering_channels: constants::MAXIMUM_ORDERING_CHANNELS as usize,
            ack_queue_capacity: 1024,
            split_timeout: Duration::from_secs(30),
            reliable_window: constants::MAX_ACK_SEQUENCES as u32,
            max_split_parts: 8192,
            max_concurrent_splits: 4096,
        }
    }
}

impl RaknetListenerConfig {
    /// Creates a new [`RaknetListenerConfig`] with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a builder for [`RaknetListenerConfig`].
    pub fn builder() -> RaknetListenerConfigBuilder {
        RaknetListenerConfigBuilder::default()
    }
}

/// Configuration builder for [`RaknetListener`].
#[derive(Debug, Clone)]
pub struct RaknetListenerConfigBuilder {
    bind_addr: Option<SocketAddr>,
    max_connections: usize,
    max_pending_connections: usize,
    max_mtu: u16,
    socket_recv_buffer_size: Option<usize>,
    socket_send_buffer_size: Option<usize>,
    session_timeout: Duration,
    session_stale: Duration,
    max_queued_reliable_bytes: usize,
    advertisement: Vec<u8>,
    max_ordering_channels: usize,
    ack_queue_capacity: usize,
    split_timeout: Duration,
    reliable_window: u32,
    max_split_parts: u32,
    max_concurrent_splits: usize,
}

impl Default for RaknetListenerConfigBuilder {
    fn default() -> Self {
        let config = RaknetListenerConfig::default();
        Self {
            bind_addr: config.bind_addr,
            max_connections: config.max_connections,
            max_pending_connections: config.max_pending_connections,
            max_mtu: config.max_mtu,
            socket_recv_buffer_size: config.socket_recv_buffer_size,
            socket_send_buffer_size: config.socket_send_buffer_size,
            session_timeout: config.session_timeout,
            session_stale: config.session_stale,
            max_queued_reliable_bytes: config.max_queued_reliable_bytes,
            advertisement: config.advertisement,
            max_ordering_channels: config.max_ordering_channels,
            ack_queue_capacity: config.ack_queue_capacity,
            split_timeout: config.split_timeout,
            reliable_window: config.reliable_window,
            max_split_parts: config.max_split_parts,
            max_concurrent_splits: config.max_concurrent_splits,
        }
    }
}

impl From<RaknetListenerConfigBuilder> for RaknetListenerConfig {
    fn from(builder: RaknetListenerConfigBuilder) -> Self {
        builder.build()
    }
}

impl RaknetListenerConfigBuilder {
    /// Creates a new [`RaknetListenerConfigBuilder`] with default values.
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets the bind address.
    #[must_use]
    pub fn bind_address(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.bind_addr = Some(addr.into());
        self
    }

    /// Sets the maximum number of concurrent connections.
    pub fn max_connections(mut self, value: usize) -> Self {
        self.max_connections = value;
        self
    }

    /// Sets the maximum number of pending connections.
    pub fn max_pending_connections(mut self, value: usize) -> Self {
        self.max_pending_connections = value;
        self
    }

    /// Sets the maximum MTU size.
    pub fn max_mtu(mut self, mtu: u16) -> Self {
        self.max_mtu = mtu;
        self
    }

    /// Sets the socket receive buffer size.
    pub fn socket_recv_buffer_size(mut self, size: Option<usize>) -> Self {
        self.socket_recv_buffer_size = size;
        self
    }

    /// Sets the socket send buffer size.
    pub fn socket_send_buffer_size(mut self, size: Option<usize>) -> Self {
        self.socket_send_buffer_size = size;
        self
    }

    /// Sets the session inactivity timeout.
    pub fn session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = timeout;
        self
    }

    /// Sets the session stale duration.
    pub fn session_stale(mut self, duration: Duration) -> Self {
        self.session_stale = duration;
        self
    }

    /// Sets the maximum queued reliable bytes per session.
    pub fn max_queued_reliable_bytes(mut self, bytes: usize) -> Self {
        self.max_queued_reliable_bytes = bytes;
        self
    }

    /// Sets the advertisement payload.
    pub fn advertisement(mut self, advertisement: impl Into<Vec<u8>>) -> Self {
        self.advertisement = advertisement.into();
        self
    }

    /// Sets the maximum number of ordering channels.
    pub fn max_ordering_channels(mut self, channels: usize) -> Self {
        self.max_ordering_channels = channels;
        self
    }

    /// Sets the ACK queue capacity.
    pub fn ack_queue_capacity(mut self, capacity: usize) -> Self {
        self.ack_queue_capacity = capacity;
        self
    }

    /// Sets the timeout for split packet reassembly.
    pub fn split_timeout(mut self, timeout: Duration) -> Self {
        self.split_timeout = timeout;
        self
    }

    /// Sets the reliable window size.
    pub fn reliable_window(mut self, window: u32) -> Self {
        self.reliable_window = window;
        self
    }

    /// Sets the maximum number of parts in a split packet.
    pub fn max_split_parts(mut self, parts: u32) -> Self {
        self.max_split_parts = parts;
        self
    }

    /// Sets the maximum number of concurrent split packets.
    pub fn max_concurrent_splits(mut self, splits: usize) -> Self {
        self.max_concurrent_splits = splits;
        self
    }

    /// Builds the [`RaknetListenerConfig`].
    pub fn build(self) -> RaknetListenerConfig {
        RaknetListenerConfig {
            bind_addr: self.bind_addr,
            max_connections: self.max_connections,
            max_pending_connections: self.max_pending_connections,
            max_mtu: self.max_mtu,
            socket_recv_buffer_size: self.socket_recv_buffer_size,
            socket_send_buffer_size: self.socket_send_buffer_size,
            session_timeout: self.session_timeout,
            session_stale: self.session_stale,
            max_queued_reliable_bytes: self.max_queued_reliable_bytes,
            advertisement: self.advertisement,
            max_ordering_channels: self.max_ordering_channels,
            ack_queue_capacity: self.ack_queue_capacity,
            split_timeout: self.split_timeout,
            reliable_window: self.reliable_window,
            max_split_parts: self.max_split_parts,
            max_concurrent_splits: self.max_concurrent_splits,
        }
    }
}

/// Server-side RakNet listener that accepts new connections.
pub struct RaknetListener {
    local_addr: SocketAddr,
    new_connections: mpsc::Receiver<(
        SocketAddr,
        mpsc::Receiver<Result<super::ReceivedMessage, crate::RaknetError>>,
    )>,
    outbound_tx: mpsc::Sender<super::OutboundMsg>,
    advertisement: Arc<RwLock<Vec<u8>>>,
}

impl RaknetListener {
    /// Binds a new listener to the specified address using the provided configuration.
    pub async fn bind(config: RaknetListenerConfig) -> std::io::Result<Self> {
        let addr = config.bind_addr.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "bind_addr is required in config",
            )
        })?;
        let socket = UdpSocket::bind(addr).await?;
        // if let Some(size) = config.socket_recv_buffer_size {
        //     let _ = socket.set_recv_buffer_size(size);
        // }
        // if let Some(size) = config.socket_send_buffer_size {
        //     let _ = socket.set_send_buffer_size(size);
        // }

        let local_addr = socket.local_addr()?;
        let (new_conn_tx, new_conn_rx) = mpsc::channel(32);
        let (outbound_tx, outbound_rx) = mpsc::channel(1024);
        let advertisement = Arc::new(RwLock::new(config.advertisement.clone()));

        tokio::spawn(run_listener_muxer(
            socket,
            config,
            new_conn_tx,
            outbound_rx,
            advertisement.clone(),
        ));

        Ok(Self {
            local_addr,
            new_connections: new_conn_rx,
            outbound_tx,
            advertisement,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Accepts the next incoming connection.
    pub async fn accept(&mut self) -> Option<RaknetStream> {
        let (peer, incoming) = self.new_connections.recv().await?;

        Some(RaknetStream::new(
            self.local_addr,
            peer,
            incoming,
            self.outbound_tx.clone(),
        ))
    }

    /// Sets the advertisement data (Pong payload) sent in response to UnconnectedPing (0x01) and OpenConnections (0x02).
    pub fn set_advertisement(&self, data: Vec<u8>) {
        if let Ok(mut guard) = self.advertisement.write() {
            *guard = data;
        }
    }

    /// Gets a copy of the current advertisement data.
    pub fn get_advertisement(&self) -> Vec<u8> {
        self.advertisement
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

async fn run_listener_muxer(
    socket: UdpSocket,

    config: RaknetListenerConfig,

    new_conn_tx: mpsc::Sender<(
        SocketAddr,
        mpsc::Receiver<Result<super::ReceivedMessage, crate::RaknetError>>,
    )>,

    mut outbound_rx: mpsc::Receiver<super::OutboundMsg>,

    advertisement: Arc<RwLock<Vec<u8>>>,
) {
    // Allocate a receive buffer large enough to avoid OS "message too long" errors even if a peer
    // sends a slightly larger probe than our configured MTU.
    let mut buf = vec![0u8; (config.max_mtu as usize + UDP_HEADER_SIZE + 64).max(2048)];
    let mut sessions: HashMap<SocketAddr, SessionState> = HashMap::new();
    let mut pending: HashMap<SocketAddr, PendingConnection> = HashMap::new();
    let mut tick = new_tick_interval();

    loop {
        tokio::select! {
            res = socket.recv_from(&mut buf) => {
                match res  {
                    Ok((len, peer)) => {
                        dispatch_datagram(
                            &socket,
                            &config,
                            &buf[..len],
                            peer,
                            &mut sessions,
                            &mut pending,
                            &new_conn_tx,
                            &advertisement,

                        ).await;
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::ConnectionReset {
                            // Windows ICMP port unreachable - ignore
                            continue;
                        }
                        tracing::error!("UDP socket error: {}", e);
                        // Don't break on transient errors
                        continue;
                    }
                }
            }
            Some(msg) = outbound_rx.recv() => {
                handle_outgoing_msg(&socket, config.max_mtu as usize, msg, &mut sessions, &config).await;
            }
            _ = tick.tick() => {
                tick_sessions(&socket, &mut sessions).await;

            }
        }
    }
}
