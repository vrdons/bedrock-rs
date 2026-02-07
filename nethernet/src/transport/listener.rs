use crate::error::{NethernetError, Result};
use crate::protocol::{
    Signal, SignalType,
    constants::{RELIABLE_CHANNEL, UNRELIABLE_CHANNEL},
};
use crate::session::Session;
use crate::signaling::Signaling;
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use webrtc::api::APIBuilder;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::peer_connection::configuration::RTCConfiguration;

/// NetherNet listener - accepts WebRTC connections
pub struct NethernetListener<S: Signaling> {
    incoming: Arc<Mutex<mpsc::UnboundedReceiver<Arc<Session>>>>,
    local_addr: SocketAddr,
    cancel_token: CancellationToken,
    _signal_handler_task: JoinHandle<()>,
    _phantom: PhantomData<S>,
}

impl<S: Signaling + 'static> NethernetListener<S> {
    /// Create a new NethernetListener bound to the given local address using the provided signaling implementation.
    ///
    /// The returned listener is ready to accept inbound WebRTC sessions. It initializes internal queues and dispatch
    /// structures, and spawns a background task to process signaling events; dropping the listener cancels that task.
    ///
    /// # Parameters
    ///
    /// - `signaling`: Signaling implementation used to exchange WebRTC offers, answers, and ICE candidates.
    /// - `local_addr`: Socket address that the listener reports as its local endpoint.
    ///
    /// # Returns
    ///
    /// A configured `NethernetListener` able to yield incoming `Session` instances.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::net::SocketAddr;
    /// # use std::sync::Arc;
    /// # use nethernet::transport::listener::NethernetListener;
    /// # // `MySignaling` must implement the `Signaling` trait.
    /// # async fn example(signaling: impl nethernet::signaling::Signaling + 'static, addr: SocketAddr) {
    /// let listener = NethernetListener::bind(signaling, addr).await.unwrap();
    /// assert_eq!(listener.local_addr(), addr);
    /// # }
    /// ```
    pub async fn bind(signaling: S, local_addr: SocketAddr) -> Result<Self> {
        let signaling = Arc::new(signaling);
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let signal_dispatchers = Arc::new(Mutex::new(HashMap::new()));
        let cancel_token = CancellationToken::new();

        // Start signal handler task
        let signal_handler_task = Self::start_signal_handler(
            signaling,
            incoming_tx,
            signal_dispatchers,
            cancel_token.clone(),
        );

        let listener = Self {
            incoming: Arc::new(Mutex::new(incoming_rx)),
            local_addr,
            cancel_token,
            _signal_handler_task: signal_handler_task,
            _phantom: PhantomData,
        };

        Ok(listener)
    }

    /// Spawns a background task that processes incoming signaling messages and routes them to connection handlers.
    ///
    /// The task consumes the signaling stream and:
    /// - on an `Offer`, negotiates and establishes a new PeerConnection via `handle_offer` and enqueues the resulting `Session` on `incoming_tx`;
    /// - on `Answer`, `Candidate`, or `Error`, forwards the signal to the per-connection dispatcher channel keyed by `signal.connection_id`;
    /// - exits when the signaling stream ends or when `cancel_token` is cancelled.
    ///
    /// Parameters:
    /// - `signaling`: shared signaling implementation that provides a stream of `Signal` items.
    /// - `incoming_tx`: channel used to deliver newly created `Session` instances to the listener.
    /// - `signal_dispatchers`: map of per-connection signal channels used to forward answers/candidates/errors to the associated connection handler.
    /// - `cancel_token`: token used to request graceful shutdown of the background task.
    ///
    /// # Returns
    ///
    /// A `JoinHandle<()>` for the spawned background task.
    fn start_signal_handler(
        signaling: Arc<S>,
        incoming_tx: mpsc::UnboundedSender<Arc<Session>>,
        signal_dispatchers: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Signal>>>>,
        cancel_token: CancellationToken,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut signals = signaling.signals();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    signal = signals.next() => {
                        match signal {
                            Some(signal) => {
                                match signal.signal_type {
                                    SignalType::Offer => {
                                        if let Err(e) = Self::handle_offer(
                                            signal,
                                            &signaling,
                                            &incoming_tx,
                                            &signal_dispatchers,
                                        )
                                        .await
                                        {
                                            tracing::debug!("Failed to handle offer: {}", e);
                                        }
                                    }
                                    SignalType::Answer | SignalType::Candidate | SignalType::Error => {
                                        // Dispatch to per-connection channel
                                        let dispatchers = signal_dispatchers.lock().await;
                                        if let Some(tx) = dispatchers.get(&signal.connection_id) {
                                            let _ = tx.send(signal);
                                        }
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        })
    }

    /// Handles an incoming Offer signal: negotiates a PeerConnection, exchanges ICE candidates and the SDP answer,
    /// registers a per-connection signal dispatcher, attaches data-channel handlers, and enqueues the established Session.
    ///
    /// Performs these actions:
    /// - Builds a WebRTC API and PeerConnection configured to avoid IPv6 link-local binding issues.
    /// - Wires generated ICE candidates back through `signaling`.
    /// - Applies the remote offer, creates and signals an answer.
    /// - Inserts a per-connection dispatcher into `signal_dispatchers` and spawns a task to feed incoming ICE candidates
    ///   from that dispatcher into the PeerConnection.
    /// - Attaches a connection-state watcher that removes the dispatcher on terminal states and a data-channel handler
    ///   that assigns opened channels to the new Session.
    /// - Sends the created Session into `incoming_tx`.
    ///
    /// # Parameters
    /// - `signal`: The incoming Offer `Signal` (contains `connection_id`, `network_id`, and SDP in `data`).
    /// - `signaling`: Shared signaling implementation used to send Answer and Candidate signals.
    /// - `incoming_tx`: Channel sender where the newly established `Session` is enqueued for acceptors.
    /// - `signal_dispatchers`: Shared map of per-connection signal senders keyed by `connection_id`; this function
    ///   inserts and later removes the per-connection sender as part of connection lifecycle.
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if WebRTC setup, SDP handling, or signaling fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use tokio::sync::mpsc;
    /// # async fn example() -> anyhow::Result<()> {
    /// // Placeholder values — replace with real Signal, signaling implementation,
    /// // incoming channel sender, and shared dispatchers map.
    /// // let signal = ...;
    /// // let signaling = Arc::new(MySignaling::new());
    /// // let (tx, _rx) = mpsc::unbounded_channel();
    /// // let dispatchers = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
    ///
    /// // Await handling of an offer:
    /// // handle_offer(signal, &signaling, &tx, &dispatchers).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn handle_offer(
        signal: Signal,
        signaling: &Arc<S>,
        incoming_tx: &mpsc::UnboundedSender<Arc<Session>>,
        signal_dispatchers: &Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Signal>>>>,
    ) -> Result<()> {
        // Create WebRTC API with custom settings
        let media_engine = MediaEngine::default();

        // Configure SettingEngine to avoid IPv6 link-local binding issues
        let mut setting_engine = SettingEngine::default();
        // Reject only IPv6 link-local addresses (fe80::/10) to avoid binding errors on Linux
        setting_engine.set_ip_filter(Box::new(|ip| {
            match ip {
                std::net::IpAddr::V6(v6) => {
                    let octets = v6.octets();
                    // Reject IPv6 link-local addresses (fe80::/10)
                    !(octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
                }
                _ => true, // Allow IPv4 and other addresses
            }
        }));

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .build();

        // Create peer connection with mDNS candidates for LAN
        let config = RTCConfiguration {
            ice_servers: vec![],
            ..Default::default()
        };

        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        // Set up ICE candidate handler
        let signaling_clone = signaling.clone();
        let network_id_clone = signal.network_id.clone();
        let connection_id = signal.connection_id;
        peer_connection.on_ice_candidate(Box::new(
            move |candidate: Option<webrtc::ice_transport::ice_candidate::RTCIceCandidate>| {
                let signaling = signaling_clone.clone();
                let network_id = network_id_clone.clone();
                Box::pin(async move {
                    if let Some(candidate) = candidate {
                        if let Ok(json) = candidate.to_json() {
                            // Serialize full RTCIceCandidateInit (includes candidate, sdp_mid, sdp_mline_index)
                            if let Ok(candidate_json) = serde_json::to_string(&json) {
                                let candidate_signal =
                                    Signal::candidate(connection_id, candidate_json, network_id);
                                let _ = signaling.signal(candidate_signal).await;
                            }
                        }
                    }
                })
            },
        ));

        // Set remote description (offer)
        let offer =
            webrtc::peer_connection::sdp::session_description::RTCSessionDescription::offer(
                signal.data.clone(),
            )?;
        peer_connection.set_remote_description(offer).await?;

        // Create answer
        let answer = peer_connection.create_answer(None).await?;
        peer_connection
            .set_local_description(answer.clone())
            .await?;

        // Signal the answer
        let answer_signal =
            Signal::answer(signal.connection_id, answer.sdp, signal.network_id.clone());
        signaling.signal(answer_signal).await?;

        // Register per-connection signal channel
        let (signal_tx, mut signal_rx) = mpsc::unbounded_channel();
        {
            let mut dispatchers = signal_dispatchers.lock().await;
            dispatchers.insert(connection_id, signal_tx);
        }

        // Set up connection state change handler to clean up dispatcher
        let signal_dispatchers_for_cleanup = signal_dispatchers.clone();
        peer_connection.on_peer_connection_state_change(Box::new(move |state| {
            let signal_dispatchers = signal_dispatchers_for_cleanup.clone();
            Box::pin(async move {
                use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
                match state {
                    RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Closed => {
                        // Remove dispatcher to allow signal_tx to drop and signal_rx to close
                        let mut dispatchers = signal_dispatchers.lock().await;
                        dispatchers.remove(&connection_id);
                    }
                    _ => {}
                }
            })
        }));

        // Handle incoming ICE candidates for this connection
        let peer_connection_clone = peer_connection.clone();
        let signal_dispatchers_clone = signal_dispatchers.clone();
        tokio::spawn(async move {
            while let Some(sig) = signal_rx.recv().await {
                if sig.signal_type == SignalType::Candidate {
                    // Deserialize full RTCIceCandidateInit (includes candidate, sdp_mid, sdp_mline_index)
                    if let Ok(candidate_init) = serde_json::from_str::<
                        webrtc::ice_transport::ice_candidate::RTCIceCandidateInit,
                    >(&sig.data)
                    {
                        let _ = peer_connection_clone
                            .add_ice_candidate(candidate_init)
                            .await;
                    }
                }
            }
            // Clean up dispatcher when channel closes (if not already removed by state handler)
            let mut dispatchers = signal_dispatchers_clone.lock().await;
            dispatchers.remove(&connection_id);
        });

        let session = Arc::new(Session::new(peer_connection.clone()));

        // Event handler for data channels
        let session_clone = session.clone();
        peer_connection.on_data_channel(Box::new(move |channel| {
            let session = session_clone.clone();
            Box::pin(async move {
                let label = channel.label().to_string();
                match label.as_str() {
                    RELIABLE_CHANNEL => {
                        let _ = session.set_reliable_channel(channel).await;
                    }
                    UNRELIABLE_CHANNEL => {
                        let _ = session.set_unreliable_channel(channel).await;
                    }
                    _ => {}
                }
            })
        }));

        // Add as incoming connection
        let _ = incoming_tx.send(session);

        Ok(())
    }

    /// Waits for and returns the next inbound session.
    ///
    /// Returns `Ok(Arc<Session>)` when a new connection is available. Returns
    /// `Err(NethernetError::ConnectionClosed)` if the listener's incoming channel has closed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # tokio_test::block_on(async {
    /// // `listener` must be a previously bound `NethernetListener`.
    /// let session: Arc<nethernet::transport::session::Session> = listener.accept().await.unwrap();
    /// # });
    /// ```
    pub async fn accept(&self) -> Result<Arc<Session>> {
        let mut incoming = self.incoming.lock().await;
        incoming
            .recv()
            .await
            .ok_or_else(|| NethernetError::ConnectionClosed)
    }

    /// Local socket address that this listener is bound to.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::net::SocketAddr;
    /// # use std::str::FromStr;
    /// # // assume `listener` is a `NethernetListener<impl Signaling>`
    /// # fn example(listener: &crate::transport::listener::NethernetListener<impl crate::signaling::Signaling>) {
    /// let addr: SocketAddr = listener.local_addr();
    /// println!("listening on {}", addr);
    /// # }
    /// ```
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl<S: Signaling> Drop for NethernetListener<S> {
    /// Cancels the listener's shutdown token to stop the background signal-handling task.
    ///
    /// This triggers graceful termination of the task that processes incoming signaling events.
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl<S: Signaling + 'static> Stream for NethernetListener<S> {
    type Item = Arc<Session>;

    /// Polls the listener for the next inbound session, returning Pending if the internal queue lock is contended.
    ///
    /// This attempts to acquire the internal incoming-session mutex without waiting; if the lock is held by
    /// another task, the method returns `Poll::Pending`. When the lock is acquired, it delegates to the
    /// inner receiver's poll to produce the next `Arc<Session>`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use futures::StreamExt;
    /// # async fn _example(mut s: impl futures::Stream<Item = std::sync::Arc<crate::Session>> + Unpin) {
    /// let _next = s.next().await; // awaits the next inbound session
    /// # }
    /// ```
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut incoming = match self.incoming.try_lock() {
            Ok(guard) => guard,
            Err(_) => return Poll::Pending,
        };

        Pin::new(&mut *incoming).poll_recv(cx)
    }
}