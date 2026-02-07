use crate::error::{NethernetError, Result};
use crate::protocol::{
    Signal, SignalType,
    constants::{RELIABLE_CHANNEL, UNRELIABLE_CHANNEL},
};
use crate::session::Session;
use crate::signaling::Signaling;
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{Mutex, mpsc};
use webrtc::api::APIBuilder;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::peer_connection::configuration::RTCConfiguration;

/// NetherNet listener - accepts WebRTC connections
pub struct NethernetListener<S: Signaling> {
    signaling: Arc<S>,
    incoming: Arc<Mutex<mpsc::UnboundedReceiver<Arc<Session>>>>,
    incoming_tx: mpsc::UnboundedSender<Arc<Session>>,
    local_addr: SocketAddr,
    /// Map of connection IDs to per-connection signal senders
    signal_dispatchers: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Signal>>>>,
}

impl<S: Signaling + 'static> NethernetListener<S> {
    /// Creates a new listener
    pub async fn bind(signaling: S, local_addr: SocketAddr) -> Result<Self> {
        let signaling = Arc::new(signaling);
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let signal_dispatchers = Arc::new(Mutex::new(HashMap::new()));

        let listener = Self {
            signaling: signaling.clone(),
            incoming: Arc::new(Mutex::new(incoming_rx)),
            incoming_tx,
            local_addr,
            signal_dispatchers: signal_dispatchers.clone(),
        };

        // Start signal handler
        listener.start_signal_handler();

        Ok(listener)
    }

    fn start_signal_handler(&self) {
        let signaling = self.signaling.clone();
        let incoming_tx = self.incoming_tx.clone();
        let signal_dispatchers = self.signal_dispatchers.clone();

        tokio::spawn(async move {
            let mut signals = signaling.signals();

            while let Some(signal) = signals.next().await {
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
        });
    }

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

    /// Accepts a new connection
    pub async fn accept(&self) -> Result<Arc<Session>> {
        let mut incoming = self.incoming.lock().await;
        incoming
            .recv()
            .await
            .ok_or_else(|| NethernetError::ConnectionClosed)
    }

    /// Returns the local address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl<S: Signaling + 'static> Stream for NethernetListener<S> {
    type Item = Arc<Session>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut incoming = match self.incoming.try_lock() {
            Ok(guard) => guard,
            Err(_) => return Poll::Pending,
        };

        Pin::new(&mut *incoming).poll_recv(cx)
    }
}
