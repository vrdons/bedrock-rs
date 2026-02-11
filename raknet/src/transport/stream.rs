use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};

use bytes::{BufMut, Bytes, BytesMut};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior, timeout};
use tokio_util::codec::BytesCodec;
use tokio_util::sync::CancellationToken;
use tokio_util::udp::UdpFramed;
use futures::{SinkExt, Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::protocol::{
    constants::{
        DEFAULT_SENT_DATAGRAM_TIMEOUT, DEFAULT_UNCONNECTED_MAGIC, MAX_ACK_SEQUENCES,
        MAXIMUM_MTU_SIZE, MAXIMUM_ORDERING_CHANNELS, MINIMUM_MTU_SIZE, RAKNET_PROTOCOL_VERSION,
    },
    datagram::Datagram,
    packet::RaknetPacket,
    types::EoBPadding,
};
use crate::session::manager::{ConnectionState, ManagedSession, SessionConfig, SessionRole};

use super::{OutboundMsg, ReceivedMessage};

const TICK_INTERVAL: Duration = Duration::from_millis(20);
const HANDSHAKE_RETRIES: usize = 3;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(400);

/// Configuration for [`RaknetStream`].
#[derive(Debug, Clone)]
pub struct RaknetStreamConfig {
    /// The address to connect to.
    pub connect_addr: SocketAddr,
    /// MTU size to attempt negotiation with.
    pub mtu: u16,
    /// Timeout for the initial connection handshake.
    pub connection_timeout: Duration,
    /// Timeout for an active session.
    pub session_timeout: Duration,
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
    /// Maximum number of tracked sent datagrams.
    pub max_sent_datagrams: Option<usize>,
    /// Timeout for tracked sent datagrams.
    pub sent_datagram_timeout: Option<Duration>,
}

impl Default for RaknetStreamConfig {
    /// Construct a [`RaknetStreamConfig`] populated with the library's default values.
    fn default() -> Self {
        Self {
            connect_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 19132)),
            mtu: 1400,
            connection_timeout: Duration::from_secs(10),
            session_timeout: Duration::from_secs(10),
            max_ordering_channels: MAXIMUM_ORDERING_CHANNELS as usize,
            ack_queue_capacity: 1024,
            split_timeout: Duration::from_secs(30),
            reliable_window: MAX_ACK_SEQUENCES as u32,
            max_split_parts: 8192,
            max_concurrent_splits: 4096,
            max_sent_datagrams: None,
            sent_datagram_timeout: None,
        }
    }
}

impl From<RaknetStreamConfigBuilder> for RaknetStreamConfig {
    fn from(builder: RaknetStreamConfigBuilder) -> Self {
        builder.build()
    }
}

impl RaknetStreamConfig {
    /// Creates a new [`RaknetStreamConfig`] with default values.
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates a builder for [`RaknetStreamConfig`].
    pub fn builder() -> RaknetStreamConfigBuilder {
        RaknetStreamConfigBuilder::default()
    }
}

/// Configuration builder for [`RaknetStream`].
#[derive(Debug, Clone)]
pub struct RaknetStreamConfigBuilder {
    connect_addr: Option<SocketAddr>,
    mtu: u16,
    connection_timeout: Duration,
    session_timeout: Duration,
    max_ordering_channels: usize,
    ack_queue_capacity: usize,
    split_timeout: Duration,
    reliable_window: u32,
    max_split_parts: u32,
    max_concurrent_splits: usize,
    max_sent_datagrams: Option<usize>,
    sent_datagram_timeout: Option<Duration>,
}

impl Default for RaknetStreamConfigBuilder {
    /// Creates a [`RaknetStreamConfigBuilder`] pre-populated with the library's default RakNet stream settings.
    fn default() -> Self {
        let config = RaknetStreamConfig::default();
        Self {
            connect_addr: None,
            mtu: config.mtu,
            connection_timeout: config.connection_timeout,
            session_timeout: config.session_timeout,
            max_ordering_channels: config.max_ordering_channels,
            ack_queue_capacity: config.ack_queue_capacity,
            split_timeout: config.split_timeout,
            reliable_window: config.reliable_window,
            max_split_parts: config.max_split_parts,
            max_concurrent_splits: config.max_concurrent_splits,
            max_sent_datagrams: config.max_sent_datagrams,
            sent_datagram_timeout: config.sent_datagram_timeout,
        }
    }
}

impl RaknetStreamConfigBuilder {
    /// Creates a new [`RaknetStreamConfigBuilder`] with default values.
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    /// Sets the address to connect to.
    pub fn connect_addr(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.connect_addr = Some(addr.into());
        self
    }

    /// Set the MTU (maximum transmission unit), in bytes, to propose during the connection handshake.
    ///
    /// The specified value is used when negotiating packet fragmentation and framing with the remote peer.
    pub fn mtu(mut self, mtu: u16) -> Self {
        self.mtu = mtu;
        self
    }

    /// Set the initial handshake timeout used when attempting to establish a connection.
    ///
    /// Returns the builder with the updated timeout.
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Sets the session inactivity timeout.
    pub fn session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = timeout;
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

    /// Sets the timeout for reassembling split packets.
    pub fn split_timeout(mut self, timeout: Duration) -> Self {
        self.split_timeout = timeout;
        self
    }

    /// Sets the maximum reliable window size.
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

    /// Sets the maximum number of tracked sent datagrams.
    ///
    /// If not set, defaults to `reliable_window` size.
    pub fn max_sent_datagrams(mut self, max: usize) -> Self {
        self.max_sent_datagrams = Some(max);
        self
    }

    /// Sets the timeout for tracked sent datagrams.
    ///
    /// If not set, defaults to 10 seconds.
    pub fn sent_datagram_timeout(mut self, timeout: Duration) -> Self {
        self.sent_datagram_timeout = Some(timeout);
        self
    }

    /// Constructs a `RaknetStreamConfig` from this builder.
    ///
    /// The resulting config copies all tunable fields from the builder. The builder must have
    /// a `connect_addr` set prior to calling this method; the function will panic if `connect_addr`
    /// is missing.
    pub fn build(self) -> RaknetStreamConfig {
        let server = self
            .connect_addr
            .ok_or_else(|| crate::RaknetError::MissingConfigValue("connect_addr".to_string()))
            .unwrap();
        RaknetStreamConfig {
            connect_addr: server,
            mtu: self.mtu,
            connection_timeout: self.connection_timeout,
            session_timeout: self.session_timeout,
            max_ordering_channels: self.max_ordering_channels,
            ack_queue_capacity: self.ack_queue_capacity,
            split_timeout: self.split_timeout,
            reliable_window: self.reliable_window,
            max_split_parts: self.max_split_parts,
            max_concurrent_splits: self.max_concurrent_splits,
            max_sent_datagrams: self.max_sent_datagrams,
            sent_datagram_timeout: self.sent_datagram_timeout,
        }
    }
}

/// A unified RakNet connection stream.
///
/// This struct represents a connection to a remote peer, whether initiated locally (client)
/// or accepted from a listener (server). It provides methods to send and receive data.
pub struct RaknetStream {
    local: SocketAddr,
    peer: SocketAddr,
    incoming: mpsc::Receiver<Result<ReceivedMessage, crate::RaknetError>>,
    outbound_tx: mpsc::Sender<OutboundMsg>,
    cancel_token: CancellationToken,
    _muxer_task: Option<JoinHandle<()>>,
}

impl RaknetStream {
    /// Internal constructor for creating a stream from an established connection.
    pub(crate) fn new(
        local: SocketAddr,
        peer: SocketAddr,
        incoming: mpsc::Receiver<Result<ReceivedMessage, crate::RaknetError>>,
        outbound_tx: mpsc::Sender<OutboundMsg>,
        cancel_token: CancellationToken,
        muxer_task: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            local,
            peer,
            incoming,
            outbound_tx,
            cancel_token,
            _muxer_task: muxer_task,
        }
    }

    /// Establishes a Raknet connection to the configured server and prepares a stream for sending and receiving messages.
    ///
    /// This performs the offline handshake (negotiating MTU and server GUID), spawns the background client muxer task, and returns a ready-to-use RaknetStream that delivers inbound messages and accepts outbound messages via its channels.
    pub async fn connect(config: RaknetStreamConfig) -> Result<Self, crate::RaknetError> {
        let server = config.connect_addr;
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let socket = UdpSocket::bind(bind_addr).await?;
        let local = socket.local_addr()?;

        // Perform offline handshake using OpenConnectionRequest1/2.
        let client_guid = client_guid();
        let handshake =
            perform_offline_handshake(&socket, server, config.mtu as usize, client_guid).await?;

        // Use negotiated MTU
        let mut config = config;
        config.mtu = handshake.mtu;

        let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundMsg>(1024);
        let (to_app_tx, to_app_rx) =
            mpsc::channel::<Result<ReceivedMessage, crate::RaknetError>>(128);
        let (ready_tx, ready_rx) = oneshot::channel();

        let cancel_token = CancellationToken::new();

        let context = ClientMuxerContext {
            server,
            server_guid: handshake.server_guid,
            client_guid, // Use the same guid generated above
            secure_connection_established: handshake.secure_connection_established,
            outbound_rx,
            to_app: to_app_tx,
            ready: ready_tx,
            config,
            cancel_token: cancel_token.clone(),
        };

        let muxer_task = tokio::spawn(run_client_muxer(socket, context));

        match ready_rx.await {
            Ok(Ok(())) => Ok(Self {
                local,
                peer: server,
                incoming: to_app_rx,
                outbound_tx,
                cancel_token,
                _muxer_task: Some(muxer_task),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(crate::RaknetError::ConnectionAborted),
        }
    }

    /// Returns the local address this stream is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Returns the address of the remote peer.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
    }

    pub async fn recv(&mut self) -> Option<Result<Bytes, crate::RaknetError>> {
        match self.recv_msg().await? {
            Ok(msg) => Some(Ok(msg.buffer)),
            Err(e) => Some(Err(e)),
        }
    }

    pub async fn recv_msg(&mut self) -> Option<Result<ReceivedMessage, crate::RaknetError>> {
        self.next().await
    }

    /// Send a message to the connected peer.
    ///
    /// Empty messages are ignored; non-empty messages are packaged as a `RaknetPacket::UserData`
    /// and forwarded to the muxer's outbound channel for delivery.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `RaknetError::ConnectionClosed` if the outbound channel is closed.
    pub async fn send(&self, msg: impl Into<super::Message>) -> Result<(), crate::RaknetError> {
        let msg = msg.into();
        let bytes = msg.buffer;

        if bytes.is_empty() {
            return Ok(());
        }
        let id = bytes[0];
        let body = bytes.slice(1..);
        self.outbound_tx
            .send(OutboundMsg {
                peer: self.peer,
                packet: RaknetPacket::UserData { id, payload: body },
                reliability: msg.reliability,
                channel: msg.channel,
                priority: msg.priority,
            })
            .await
            .map_err(|_| crate::RaknetError::ConnectionClosed)
    }
}

impl Drop for RaknetStream {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl Stream for RaknetStream {
    type Item = Result<ReceivedMessage, crate::RaknetError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.incoming.poll_recv(cx)
    }
}

pub struct OfflineHandshake {
    mtu: u16,
    server_guid: u64,
    secure_connection_established: bool,
}

struct ClientMuxerContext {
    // Connection properties
    server: SocketAddr,
    server_guid: u64,
    client_guid: u64,
    secure_connection_established: bool,

    // Communication channels
    outbound_rx: mpsc::Receiver<OutboundMsg>,
    to_app: mpsc::Sender<Result<ReceivedMessage, crate::RaknetError>>,
    ready: oneshot::Sender<Result<(), crate::RaknetError>>,
    config: RaknetStreamConfig,
    cancel_token: CancellationToken,
}

/// Runs the background muxer loop that manages a client-side RakNet session over a UDP socket.
#[tracing::instrument(skip(socket, context), fields(server = %context.server, mtu = context.config.mtu), level = "debug")]
async fn run_client_muxer(socket: UdpSocket, mut context: ClientMuxerContext) {
    let mut framed = UdpFramed::new(socket, BytesCodec::new());
    let mut managed: Option<ManagedSession> = None;
    let mut handshake_started = false;
    // We move the `ready` sender into a local Option
    let mut ready_signal = Some(context.ready);
    let mut tick = time::interval(TICK_INTERVAL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Initial handshake ensure
    {
        let now = Instant::now();
        let ms = ensure_client_session(
            &mut managed,
            context.server,
            context.config.mtu as usize,
            context.client_guid,
            now,
            &context.config,
        );
        ensure_client_handshake(
            ms,
            &mut handshake_started,
            context.server_guid,
            now,
            context.secure_connection_established,
            &mut framed,
            context.server,
        )
        .await;
    }

    loop {
        tokio::select! {
            _ = context.cancel_token.cancelled() => {
                tracing::debug!("client muxer cancelled");
                break;
            }
            res = framed.next() => {
                let (bytes, peer) = match res {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => {
                        if e.kind() == std::io::ErrorKind::ConnectionReset {
                            // On Windows, this can happen if a previous send failed (ICMP Port Unreachable).
                            // It shouldn't kill the listener loop.
                            tracing::debug!("udp connection reset (ignoring): {}", e);
                            continue;
                        }
                        tracing::error!("udp socket recv error: {}", e);
                        break;
                    }
                    None => break,
                };

                if peer != context.server {
                    tracing::debug!("ignoring packet from unknown peer");
                    continue;
                }

                if bytes.is_empty() {
                    continue;
                }

                // Filter out non-datagram packets (offline packets, e.g. OpenConnectionReply2)
                // Valid datagrams must have VALID (0x80), ACK (0x40), or NACK (0x20) flags.
                // Offline packets are typically ID < 0x20.
                let header_byte = bytes[0];
                if header_byte < 0x80 && (header_byte & 0x60) == 0 {
                    // Try to decode as a control packet to see if it's a connection failure
                    let mut slice = &bytes[..];
                    match RaknetPacket::decode(&mut slice) {
                        Ok(pkt) => {
                            let error = match pkt {
                                RaknetPacket::ConnectionRequestFailed(_) => {
                                    Some(crate::RaknetError::ConnectionRequestFailed)
                                }
                                RaknetPacket::AlreadyConnected(_) => {
                                    Some(crate::RaknetError::AlreadyConnected)
                                }
                                RaknetPacket::IncompatibleProtocolVersion(_) => {
                                    Some(crate::RaknetError::IncompatibleProtocolVersion)
                                }
                                RaknetPacket::NoFreeIncomingConnections(_) => {
                                    Some(crate::RaknetError::ServerFull)
                                }
                                RaknetPacket::ConnectionBanned(_) => {
                                    Some(crate::RaknetError::ConnectionBanned)
                                }
                                RaknetPacket::IpRecentlyConnected(_) => {
                                    Some(crate::RaknetError::IpRecentlyConnected)
                                }
                                _ => None,
                            };

                            if let Some(e) = error {
                                tracing::debug!(error = ?e, "received connection failure packet");
                                if let Some(tx) = ready_signal.take() {
                                    let _ = tx.send(Err(e));
                                }
                                return;
                            } else {
                                tracing::debug!("ignoring non-datagram packet");
                            }
                        }
                        Err(_) => {
                            tracing::debug!("ignoring malformed non-datagram packet");
                        }
                    }
                    continue;
                }

                let mut slice = &bytes[..];
                if let Ok(dgram) = Datagram::decode(&mut slice) {
                    let now = Instant::now();
                    // Use context fields
                    let ms = ensure_client_session(
                        &mut managed,
                        context.server,
                        context.config.mtu as usize,
                        context.client_guid,
                        now,
                        &context.config,
                    );
                    ensure_client_handshake(
                        ms,
                        &mut handshake_started,
                        context.server_guid,
                        now,
                        context.secure_connection_established,
                        &mut framed,
                        context.server
                    ).await;

                    // This logic remains the same, but was already correct
                    match ms.handle_datagram(dgram, now) {
                        Ok(pkts) => {
                            for p in ManagedSession::filter_app_packets(pkts) {
                                if let RaknetPacket::UserData { id, payload } = p.packet {
                                    tracing::trace!("received user packet");
                                    // Reconstruct full packet: [ID] + [Payload]
                                    let mut full = BytesMut::with_capacity(1 + payload.len());
                                    full.put_u8(id);
                                    full.extend_from_slice(&payload);
                                    let msg = ReceivedMessage {
                                        buffer: full.freeze(),
                                        reliability: p.reliability,
                                        channel: p.ordering_channel.unwrap_or(0),
                                    };
                                    if context.to_app.send(Ok(msg)).await.is_err() {
                                        tracing::debug!("app channel closed");
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if matches!(
                                e,
                                crate::session::manager::SessionError::Protocol(
                                    crate::protocol::packet::DecodeError::InvalidAddrVersion(_)
                                )
                            ) {
                                tracing::debug!(error = ?e, "ignoring datagram with invalid addr version");
                                continue;
                            }
                            tracing::debug!(error = ?e, "failed to handle datagram");
                        }
                    }
                    notify_client_ready(ms, &mut ready_signal);

                    if ms.state() == ConnectionState::Closed {
                        if let Some(reason) = ms.last_disconnect_reason() {
                            tracing::info!(reason = ?reason, "session disconnected");
                            let _ = context.to_app.send(Err(crate::RaknetError::Disconnected(reason))).await;
                        } else {
                            let _ = context.to_app.send(Err(crate::RaknetError::ConnectionClosed)).await;
                        }
                        return;
                    }

                    flush_built_datagrams(ms, &mut framed, context.server, now, false).await;
                } else {
                    tracing::debug!("failed to decode datagram");
                }
            }

            // Use context field
            Some(msg) = context.outbound_rx.recv() => {
                let now = Instant::now();
                let ms = ensure_client_session(
                    &mut managed,
                    context.server,
                    context.config.mtu as usize,
                    context.client_guid,
                    now,
                    &context.config,
                );
                ensure_client_handshake(
                    ms,
                    &mut handshake_started,
                    context.server_guid,
                    now,
                    context.secure_connection_established,
                    &mut framed,
                    context.server
                ).await;
                let _ = ms.queue_app_packet(
                    msg.packet,
                    msg.reliability,
                    msg.channel,
                    msg.priority,
                );
                flush_built_datagrams(ms, &mut framed, context.server, now, false).await;
                notify_client_ready(ms, &mut ready_signal);
            }

            _ = tick.tick() => {
                if let Some(ms) = managed.as_mut() {
                    let now = Instant::now();
                    flush_built_datagrams(ms, &mut framed, context.server, now, true).await;
                    notify_client_ready(ms, &mut ready_signal);
                }
            }

            else => break,
        }
    }

    // Graceful shutdown check using match to avoid nesting if-lets
    match managed {
        Some(mut ms) if ms.is_connected() => {
            tracing::debug!("channel closed, sending disconnect notification");
            let _ = ms.send_disconnect(crate::protocol::state::DisconnectReason::Disconnected);
            // Flush the disconnect packet
            flush_built_datagrams(&mut ms, &mut framed, context.server, Instant::now(), true).await;
        }
        _ => {}
    }

    tracing::debug!("client muxer terminated");
}

#[tracing::instrument(skip_all, level = "debug")]
pub async fn perform_offline_handshake(
    socket: &UdpSocket,
    server: SocketAddr,
    _mtu_hint: usize,
    client_guid: u64,
) -> std::io::Result<OfflineHandshake> {
    let mut reply1 = None;
    let mut used_mtu = 0;

    for &mtu in crate::protocol::constants::MTU_SIZES {
        tracing::debug!(mtu = mtu, "probing mtu");
        let req1 =
            RaknetPacket::OpenConnectionRequest1(crate::protocol::packet::OpenConnectionRequest1 {
                magic: DEFAULT_UNCONNECTED_MAGIC,
                protocol_version: RAKNET_PROTOCOL_VERSION,
                // IP (20) + UDP (8) + Packet ID (1) + Magic (16) + Protocol (1) = 46 bytes overhead
                padding: EoBPadding(mtu.saturating_sub(46).into()),
            });

        let mut buf = BytesMut::new();
        if req1.encode(&mut buf).is_err() {
            tracing::warn!(mtu = mtu, "failed to encode OpenConnectionRequest1");
            continue;
        }
        if let Err(e) = socket.send_to(&buf, server).await {
            tracing::warn!(mtu = mtu, error = ?e, "failed to send OpenConnectionRequest1");
            continue;
        }

        let mut tmp = [0u8; 2048];
        let mut attempts = 0;
        while attempts < HANDSHAKE_RETRIES {
            // Short timeout for each attempt so we don't stall the whole probe sequence.
            let res = timeout(HANDSHAKE_TIMEOUT, socket.recv_from(&mut tmp)).await;

            if let Ok(Ok((len, from))) = res {
                if from == server {
                    let mut slice = &tmp[..len];
                    // Cleaner pattern match without let_chains or nesting
                    if let Ok(RaknetPacket::OpenConnectionReply1(r)) =
                        RaknetPacket::decode(&mut slice)
                    {
                        tracing::debug!(
                            mtu = mtu,
                            server_mtu = r.mtu,
                            "received OpenConnectionReply1"
                        );
                        reply1 = Some(r);
                        used_mtu = mtu;
                        break;
                    } else {
                        tracing::debug!("ignoring non-reply1 packet during probe");
                    }
                } else {
                    tracing::debug!("ignoring reply from non-server during probe");
                }
            } else {
                tracing::debug!("timeout waiting for reply1 attempt");
            }
            attempts += 1;
        }

        if reply1.is_some() {
            break;
        }
        tracing::debug!(
            mtu = mtu,
            "no valid OpenConnectionReply1 received for probe"
        );
    }

    let reply1 = reply1.ok_or_else(|| {
        tracing::error!("failed to receive any OpenConnectionReply1");
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timeout waiting for OpenConnectionReply1 after probing MTUs",
        )
    })?;

    let server_mtu = reply1.mtu;
    let cookie = reply1.cookie;

    // Negotiate final MTU: min(client_probed, server_reported)
    let mtu_final = used_mtu
        .clamp(MINIMUM_MTU_SIZE, MAXIMUM_MTU_SIZE)
        .min(server_mtu);

    tracing::debug!(negotiated_mtu = mtu_final, "sending OpenConnectionRequest2");

    let req2 =
        RaknetPacket::OpenConnectionRequest2(crate::protocol::packet::OpenConnectionRequest2 {
            magic: DEFAULT_UNCONNECTED_MAGIC,
            cookie,
            client_proof: cookie.is_some(),
            server_addr: server,
            mtu: mtu_final,
            client_guid,
        });

    let mut buf2 = BytesMut::new();
    req2.encode(&mut buf2).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "failed to encode request2")
    })?;
    socket.send_to(&buf2, server).await?;

    let mut tmp = [0u8; 2048];
    let reply2 = loop {
        let res = timeout(Duration::from_secs(2), socket.recv_from(&mut tmp)).await;
        let (len, from) = match res {
            Ok(Ok(v)) => v,
            _ => {
                tracing::error!("timeout waiting for OpenConnectionReply2");
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timeout waiting for OpenConnectionReply2",
                ));
            }
        };
        if from != server {
            continue;
        }
        let mut slice = &tmp[..len];
        if let Ok(RaknetPacket::OpenConnectionReply2(r)) = RaknetPacket::decode(&mut slice) {
            tracing::debug!(server_guid = r.server_guid, "handshake complete");
            break r;
        }
    };

    Ok(OfflineHandshake {
        mtu: mtu_final,
        server_guid: reply2.server_guid,
        secure_connection_established: reply2.security,
    })
}

pub fn client_guid() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdead_beef_dead_beef)
}

fn ensure_client_session<'a>(
    managed: &'a mut Option<ManagedSession>,
    server: SocketAddr,
    mtu: usize,
    client_guid: u64,
    now: Instant,
    config: &RaknetStreamConfig,
) -> &'a mut ManagedSession {
    managed.get_or_insert_with(|| {
        ManagedSession::with_config(
            server,
            mtu,
            now,
            SessionConfig {
                role: SessionRole::Client,
                guid: client_guid,
                session_timeout: config.session_timeout,
                session: crate::session::SessionTunables {
                    max_ordering_channels: config.max_ordering_channels,
                    ack_queue_capacity: config.ack_queue_capacity,
                    split_timeout: config.split_timeout,
                    reliable_window: config.reliable_window,
                    max_split_parts: config.max_split_parts,
                    max_concurrent_splits: config.max_concurrent_splits,
                    max_sent_datagrams: config
                        .max_sent_datagrams
                        .unwrap_or(config.reliable_window as usize),
                    sent_datagram_timeout: config
                        .sent_datagram_timeout
                        .unwrap_or(DEFAULT_SENT_DATAGRAM_TIMEOUT),
                },
                ..SessionConfig::default()
            },
        )
    })
}

async fn ensure_client_handshake(
    managed: &mut ManagedSession,
    handshake_started: &mut bool,
    server_guid: u64,
    now: Instant,
    secure_connection_established: bool,
    framed: &mut UdpFramed<BytesCodec>,
    server: SocketAddr,
) {
    if !*handshake_started
        && managed
            .start_client_handshake(server_guid, now, secure_connection_established)
            .is_ok()
    {
        *handshake_started = true;
        flush_built_datagrams(managed, framed, server, now, false).await;
    }
}

#[tracing::instrument(skip_all, fields(peer= %peer, on_tick = %run_tick), level = "trace")]
async fn flush_built_datagrams(
    managed: &mut ManagedSession,
    framed: &mut UdpFramed<BytesCodec>,
    peer: SocketAddr,
    now: Instant,
    run_tick: bool,
) {
    if run_tick {
        for d in managed.on_tick(now) {
            tracing::trace!("send_tick_datagram");
            let mut out = BytesMut::new();
            d.encode(&mut out)
                .expect("Bad datagram made it into queue.");
            let _ = framed.send((out.freeze(), peer)).await;
        }
    }

    while let Some(d) = managed.build_datagram(now) {
        tracing::trace!("send_datagram");
        let mut out = BytesMut::new();
        d.encode(&mut out)
            .expect("Bad datagram made it into queue.");
        let _ = framed.send((out.freeze(), peer)).await;
    }

    // Delivery of any unblocked packets happens via drain_ready_to_app() at call sites.
}

/// Signal readiness to the connector once the managed session is connected.
#[tracing::instrument(skip(managed, ready), level = "trace")]
fn notify_client_ready(
    managed: &ManagedSession,
    ready: &mut Option<oneshot::Sender<Result<(), crate::RaknetError>>>,
) {
    if managed.is_connected() {
        if let Some(tx) = ready.take() {
            tracing::trace!("sending ready signal");
            let _ = tx.send(Ok(()));
        }
    }
}
