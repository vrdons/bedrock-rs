use crate::error::{NethernetError, Result};
use crate::protocol::{Signal, SignalType, constants::{RELIABLE_CHANNEL, UNRELIABLE_CHANNEL}};
use crate::session::{Session};
use crate::signaling::Signaling;
use bytes::Bytes;
use futures::{Stream, StreamExt, Future};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use webrtc::api::APIBuilder;
use webrtc::api::media_engine::MediaEngine;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::peer_connection::configuration::RTCConfiguration;

/// NetherNet stream - data transmission over WebRTC
pub struct NethernetStream {
    session: Arc<Session>,
    remote_addr: SocketAddr,
    pending_recv: Option<Pin<Box<dyn Future<Output = Result<Option<Bytes>>> + Send>>>,
}

impl NethernetStream {
    /// Connects to a remote address
    pub async fn connect<S: Signaling + 'static>(
        signaling: Arc<S>,
        remote_network_id: String,
        remote_addr: SocketAddr,
    ) -> Result<Self> {
        // Create WebRTC API
        let media_engine = MediaEngine::default();
        let api = APIBuilder::new().with_media_engine(media_engine).build();

        // Create peer connection
        let config = RTCConfiguration::default();

        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        // Create data channels
        let reliable_channel = peer_connection
            .create_data_channel(
                RELIABLE_CHANNEL,
                Some(RTCDataChannelInit {
                    ordered: Some(true),
                    ..Default::default()
                }),
            )
            .await?;

        let unreliable_channel = peer_connection
            .create_data_channel(
                UNRELIABLE_CHANNEL,
                Some(RTCDataChannelInit {
                    ordered: Some(false),
                    max_retransmits: Some(0),
                    ..Default::default()
                }),
            )
            .await?;

        // Generate connection ID
        let connection_id = rand::random::<u64>();
        
        // Create channels to wait for DataChannel open events
        let (reliable_open_tx, reliable_open_rx) = tokio::sync::oneshot::channel::<()>();
        let (unreliable_open_tx, unreliable_open_rx) = tokio::sync::oneshot::channel::<()>();
        
        // Set up on_open handlers for DataChannels
        let reliable_open_tx = Arc::new(tokio::sync::Mutex::new(Some(reliable_open_tx)));
        let unreliable_open_tx = Arc::new(tokio::sync::Mutex::new(Some(unreliable_open_tx)));
        
        let reliable_tx_clone = reliable_open_tx.clone();
        reliable_channel.on_open(Box::new(move || {
            let tx = reliable_tx_clone.clone();
            Box::pin(async move {
                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(());
                }
            })
        }));
        
        let unreliable_tx_clone = unreliable_open_tx.clone();
        unreliable_channel.on_open(Box::new(move || {
            let tx = unreliable_tx_clone.clone();
            Box::pin(async move {
                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(());
                }
            })
        }));

        // Set up ICE candidate handler to signal candidates
        let signaling_clone = signaling.clone();
        let remote_network_id_clone = remote_network_id.clone();
        peer_connection.on_ice_candidate(Box::new(move |candidate: Option<webrtc::ice_transport::ice_candidate::RTCIceCandidate>| {
            let signaling = signaling_clone.clone();
            let network_id = remote_network_id_clone.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    if let Ok(json) = candidate.to_json() {
                        let candidate_signal = Signal::candidate(
                            connection_id,
                            json.candidate,
                            network_id,
                        );
                        let _ = signaling.signal(candidate_signal).await;
                    }
                }
            })
        }));

        // Create offer
        let offer = peer_connection.create_offer(None).await?;
        peer_connection.set_local_description(offer.clone()).await?;

        // Signal the offer
        let offer_signal = Signal::offer(
            connection_id,
            offer.sdp,
            remote_network_id.clone(),
        );
        signaling.signal(offer_signal).await?;

        // Wait for answer and handle candidates
        let mut signals = signaling.signals();
        let answer = loop {
            if let Some(signal) = signals.next().await {
                if signal.connection_id == connection_id {
                    match signal.signal_type {
                        SignalType::Answer => {
                            break signal.data;
                        }
                        SignalType::Candidate => {
                            // Add remote ICE candidate
                            let candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                                candidate: signal.data,
                                ..Default::default()
                            };
                            let _ = peer_connection.add_ice_candidate(candidate).await;
                        }
                        _ => {}
                    }
                }
            } else {
                return Err(NethernetError::Timeout);
            }
        };

        // Set the answer
        let answer_desc = webrtc::peer_connection::sdp::session_description::RTCSessionDescription::answer(answer)?;
        peer_connection.set_remote_description(answer_desc).await?;
        
        // Continue processing candidates
        let peer_connection_clone = peer_connection.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.next().await {
                if signal.connection_id == connection_id && signal.signal_type == SignalType::Candidate {
                    let candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                        candidate: signal.data,
                        ..Default::default()
                    };
                    let _ = peer_connection_clone.add_ice_candidate(candidate).await;
                }
            }
        });

        let session = Arc::new(Session::new(peer_connection.clone()));
        
        // Set up data channels
        session.set_reliable_channel(reliable_channel).await?;
        session.set_unreliable_channel(unreliable_channel).await?;
        
        // Wait for both DataChannels to open (with timeout)
        let timeout_duration = std::time::Duration::from_secs(10);
        
        tokio::select! {
            _ = reliable_open_rx => {}
            _ = tokio::time::sleep(timeout_duration) => {
                return Err(NethernetError::Timeout);
            }
        }
        
        tokio::select! {
            _ = unreliable_open_rx => {}
            _ = tokio::time::sleep(timeout_duration) => {
                return Err(NethernetError::Timeout);
            }
        }
        
        // Wait for ICE connection to establish
        let peer_conn_clone = peer_connection.clone();
        let ice_connected = tokio::time::timeout(timeout_duration, async move {
            loop {
                let state = peer_conn_clone.ice_connection_state();
                match state {
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Connected |
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Completed => {
                        return Ok::<(), NethernetError>(());
                    }
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Failed |
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Disconnected |
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Closed => {
                        return Err(NethernetError::ConnectionClosed);
                    }
                    _ => {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        }).await;
        
        match ice_connected {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {}
        }

        Ok(Self {
            session,
            remote_addr,
            pending_recv: None,
        })
    }

    /// Creates from a session
    pub fn from_session(session: Arc<Session>, remote_addr: SocketAddr) -> Self {
        Self {
            session,
            remote_addr,
            pending_recv: None,
        }
    }

    /// Sends data
    pub async fn send(&self, data: Bytes) -> Result<()> {
        self.session.send(data).await
    }

    /// Receives data
    pub async fn recv(&self) -> Result<Option<Bytes>> {
        self.session.recv().await
    }

    /// Closes the stream
    pub async fn close(&self) -> Result<()> {
        self.session.close().await
    }

    /// Returns the remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    /// Provides access to the session
    pub fn session(&self) -> Arc<Session> {
        self.session.clone()
    }
}

impl Stream for NethernetStream {
    type Item = Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If no pending future exists, create one
        if self.pending_recv.is_none() {
            let session = self.session.clone();
            let fut = Box::pin(async move {
                session.recv().await
            });
            self.pending_recv = Some(fut);
        }

        // Poll the stored future
        if let Some(mut fut) = self.pending_recv.take() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Ok(Some(data))) => {
                    // Future completed with data, clear pending_recv
                    Poll::Ready(Some(Ok(data)))
                }
                Poll::Ready(Ok(None)) => {
                    // Future completed with None (stream ended), clear pending_recv
                    Poll::Ready(None)
                }
                Poll::Ready(Err(e)) => {
                    // Future completed with error, clear pending_recv
                    Poll::Ready(Some(Err(e)))
                }
                Poll::Pending => {
                    // Future not ready yet, store it back and return Pending
                    self.pending_recv = Some(fut);
                    Poll::Pending
                }
            }
        } else {
            // This should never happen, but handle it gracefully
            Poll::Pending
        }
    }
}
