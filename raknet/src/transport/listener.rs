mod offline;
mod online;

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::BytesCodec;
use tokio_util::sync::CancellationToken;
use tokio_util::udp::UdpFramed;

use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::builders::RaknetListenerConfigBuilder;
use crate::protocol::constants;
use crate::transport::listener_conn::SessionState;
use crate::transport::mux::new_tick_interval;
use crate::transport::stream::RaknetStream;

use offline::PendingConnection;

use online::{dispatch_datagram, handle_outgoing_msg, tick_sessions};

/// Configuration for a [`RaknetListener`].
#[derive(Debug, Clone)]
pub struct RaknetListenerConfig {
    /// Address to start server.
    pub bind_addr: SocketAddr,

    /// Maximum number of concurrent connections allowed.
    pub max_connections: usize,

    /// Maximum number of pending connections (handshakes).
    pub max_pending_connections: usize,

    /// Maximum MTU size to support/advertise.
    pub max_mtu: u16,

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

    /// Timeout for tracked sent datagrams.
    pub sent_datagram_timeout: Duration,
}

impl Default for RaknetListenerConfig {
    /// Creates a RaknetListenerConfig populated with sensible defaults.
    ///
    /// Defaults include:
    /// - bind address 0.0.0.0:19132
    /// - max connections and pending connections: 1024
    /// - max MTU: 1400
    /// - session timeout: 10s, session stale: 5s
    /// - 4 MiB per-session reliable queue
    /// - a default MCPE advertisement payload
    /// - ordering channels and reliable window set from protocol constants
    /// - split/reassembly limits tuned for typical use (8192 parts, 4096 concurrent)
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 19132)),
            max_connections: 1024,
            max_pending_connections: 1024,
            max_mtu: 1400,
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
            sent_datagram_timeout: constants::DEFAULT_SENT_DATAGRAM_TIMEOUT,
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

/// Server-side RakNet listener that accepts new connections.
pub struct RaknetListener {
    local_addr: SocketAddr,
    new_connections: mpsc::Receiver<(
        SocketAddr,
        mpsc::Receiver<Result<super::ReceivedMessage, crate::RaknetError>>,
    )>,
    outbound_tx: mpsc::Sender<super::OutboundMsg>,
    advertisement: Arc<RwLock<Vec<u8>>>,
    cancel_token: CancellationToken,
    _muxer_task: JoinHandle<()>,
}

impl RaknetListener {
    /// Creates and binds a new RaknetListener using the provided configuration.
    ///
    /// The function binds a UDP socket to `config.bind_addr`, starts the background
    /// listener muxer task, and returns a `RaknetListener` that yields incoming
    /// `RaknetStream` connections and provides an outbound message channel.
    ///
    /// # Errors
    ///
    /// Returns an `std::io::Error` if binding the socket or retrieving the local
    /// address fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::runtime::Runtime;
    /// # use raknet::transport::listener::{RaknetListener, RaknetListenerConfig};
    ///
    /// let rt = Runtime::new().unwrap();
    /// let listener = rt.block_on(async {
    ///     RaknetListener::bind(RaknetListenerConfig::default()).await.unwrap()
    /// });
    /// println!("Bound to {}", listener.local_addr());
    /// ```
    pub async fn bind(config: RaknetListenerConfig) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(config.bind_addr).await?;
        let local_addr = socket.local_addr()?;
        let (new_conn_tx, new_conn_rx) = mpsc::channel(32);
        let (outbound_tx, outbound_rx) = mpsc::channel(1024);
        let advertisement = Arc::new(RwLock::new(config.advertisement.clone()));
        let cancel_token = CancellationToken::new();

        let muxer_task = tokio::spawn(run_listener_muxer(
            socket,
            config,
            new_conn_tx,
            outbound_rx,
            advertisement.clone(),
            cancel_token.clone(),
        ));

        Ok(Self {
            local_addr,
            new_connections: new_conn_rx,
            outbound_tx,
            advertisement,
            cancel_token,
            _muxer_task: muxer_task,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Accepts the next incoming connection.
    pub async fn accept(&mut self) -> Option<RaknetStream> {
        self.next().await
    }

    /// Sets the advertisement data (Pong payload) sent in response to UnconnectedPing (0x01) and OpenConnections (0x02).
    pub fn set_advertisement(&self, data: Vec<u8>) {
        if let Ok(mut guard) = self.advertisement.write() {
            *guard = data;
        }
    }

    /// Get a copy of the current advertisement payload.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // `listener` is a `RaknetListener`.
    /// let payload: Vec<u8> = listener.get_advertisement();
    /// ```
    pub fn get_advertisement(&self) -> Vec<u8> {
        self.advertisement
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Drop for RaknetListener {
    /// Cancels the listener's cancellation token, signaling background tasks to stop.
    ///
    /// This runs when the listener is dropped to ensure the background muxer and related tasks are terminated.
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl Stream for RaknetListener {
    type Item = RaknetStream;

    /// Polls the listener for the next incoming RaknetStream connection.
    ///
    /// The returned value is `Some(RaknetStream)` when a new peer connection is available,
    /// `None` when the listener has closed and will yield no more connections, and `Pending`
    /// when no connection is currently ready.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::StreamExt;
    /// # async fn example(mut listener: raknet::transport::listener::RaknetListener) {
    /// if let Some(stream) = listener.next().await {
    ///     // handle the accepted RaknetStream
    /// }
    /// # }
    /// ```
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.new_connections.poll_recv(cx) {
            Poll::Ready(Some((peer, incoming))) => {
                // Each connection gets a child token so closing one stream doesn't cancel siblings
                // or the listener, while the listener can still cancel all children via the parent token
                Poll::Ready(Some(RaknetStream::new(
                    self.local_addr,
                    peer,
                    incoming,
                    self.outbound_tx.clone(),
                    self.cancel_token.child_token(),
                    None,
                )))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Background task that multiplexes UDP I/O, session maintenance, and outgoing messages for a RakNet listener.
///
/// This task receives datagrams from the provided UDP socket, dispatches them to session and pending-connection
/// state, sends outbound messages from the outbound channel, and performs periodic session maintenance.
/// It terminates when the provided cancellation token is cancelled.
///
/// # Examples
///
/// ```no_run
/// use tokio::net::UdpSocket;
/// use tokio::sync::mpsc;
/// use std::sync::RwLock;
/// use tokio_util::sync::CancellationToken;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
/// let config = raknet::transport::listener::RaknetListenerConfig::default();
/// let (new_conn_tx, _new_conn_rx) = mpsc::channel(8);
/// let (_outbound_tx, outbound_rx) = mpsc::channel(8);
/// let advertisement = Arc::new(RwLock::new(Vec::new()));
/// let cancel_token = CancellationToken::new();
///
/// // Spawn the listener muxer; it will run until `cancel_token` is cancelled.
/// tokio::spawn(run_listener_muxer(
///     socket,
///     config,
///     new_conn_tx,
///     outbound_rx,
///     advertisement,
///     cancel_token.clone(),
/// ));
/// # }
/// ```
async fn run_listener_muxer(
    socket: UdpSocket,

    config: RaknetListenerConfig,

    new_conn_tx: mpsc::Sender<(
        SocketAddr,
        mpsc::Receiver<Result<super::ReceivedMessage, crate::RaknetError>>,
    )>,

    mut outbound_rx: mpsc::Receiver<super::OutboundMsg>,

    advertisement: Arc<RwLock<Vec<u8>>>,

    cancel_token: CancellationToken,
) {
    // We use BytesCodec to handle raw packets efficiently.
    let mut framed = UdpFramed::new(socket, BytesCodec::new());
    let mut sessions: HashMap<SocketAddr, SessionState> = HashMap::new();
    let mut pending: HashMap<SocketAddr, PendingConnection> = HashMap::new();
    let mut tick = new_tick_interval();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                tracing::debug!("listener muxer cancelled");
                break;
            }
            res = framed.next() => {
                match res {
                    Some(Ok((bytes, peer))) => {
                         let bytes: bytes::BytesMut = bytes;
                         dispatch_datagram(
                            framed.get_ref(),
                            &config,
                            &bytes,
                            peer,
                            &mut sessions,
                            &mut pending,
                            &new_conn_tx,
                            &advertisement,
                        ).await;
                    }
                    Some(Err(e)) => {
                        tracing::error!("UdpFramed error: {}", e);
                    }
                    None => break,
                }
            }
            Some(msg) = outbound_rx.recv() => {
                handle_outgoing_msg(framed.get_ref(), config.max_mtu as usize, msg, &mut sessions, &config).await;
            }
            _ = tick.tick() => {
                tick_sessions(framed.get_ref(), &mut sessions).await;
            }
        }
    }
}
