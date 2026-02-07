use crate::error::{NethernetError, Result};
use crate::protocol::constants::DEFAULT_PACKET_CHANNEL_CAPACITY;
use crate::protocol::{Message, MessageSegment};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::peer_connection::RTCPeerConnection;

/// WebRTC session manager
pub struct Session {
    peer_connection: Arc<RTCPeerConnection>,
    reliable_channel: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    unreliable_channel: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    message_buffer: Arc<Mutex<Message>>,
    packet_tx: mpsc::Sender<Bytes>,
    packet_rx: Arc<Mutex<Option<mpsc::Receiver<Bytes>>>>,
    closed: Arc<RwLock<bool>>,
}

impl Session {
    pub fn new(peer_connection: Arc<RTCPeerConnection>) -> Self {
        Self::with_capacity(peer_connection, DEFAULT_PACKET_CHANNEL_CAPACITY)
    }

    /// Creates a new session with a custom packet channel capacity
    pub fn with_capacity(peer_connection: Arc<RTCPeerConnection>, capacity: usize) -> Self {
        let (packet_tx, packet_rx) = mpsc::channel(capacity);

        Self {
            peer_connection,
            reliable_channel: Arc::new(Mutex::new(None)),
            unreliable_channel: Arc::new(Mutex::new(None)),
            message_buffer: Arc::new(Mutex::new(Message::new())),
            packet_tx,
            packet_rx: Arc::new(Mutex::new(Some(packet_rx))),
            closed: Arc::new(RwLock::new(false)),
        }
    }

    /// Sets the reliable data channel
    pub async fn set_reliable_channel(&self, channel: Arc<RTCDataChannel>) -> Result<()> {
        let message_buffer = self.message_buffer.clone();
        let packet_tx = self.packet_tx.clone();

        channel.on_message(Box::new(move |msg| {
            let data = msg.data.clone();
            let buffer = message_buffer.clone();
            let tx = packet_tx.clone();

            Box::pin(async move {
                let data_len = data.len();
                match MessageSegment::decode(data.clone()) {
                    Ok(segment) => {
                        let result = {
                            let mut buf = buffer.lock().await;
                            buf.add_segment(segment)
                        };
                        match result {
                            Ok(Some(complete_msg)) => {
                                // Use async send to handle backpressure with bounded channel
                                // If send fails, it means the receiver has been dropped
                                let _ = tx.send(complete_msg).await;
                            }
                            Ok(None) => {
                                tracing::debug!("incomplete segment added to buffer, waiting for more segments");
                            }
                            Err(e) => {
                                tracing::warn!("failed to add segment to buffer: {:?}, data length: {}", e, data_len);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "failed to decode message segment: {:?}, data length: {}, data preview: {:?}",
                            e,
                            data_len,
                            &data.as_ref()[..data_len.min(64)]
                        );
                    }
                }
            })
        }));

        *self.reliable_channel.lock().await = Some(channel);
        Ok(())
    }

    /// Sets the unreliable data channel
    pub async fn set_unreliable_channel(&self, channel: Arc<RTCDataChannel>) -> Result<()> {
        *self.unreliable_channel.lock().await = Some(channel);
        Ok(())
    }

    /// Sends a message
    pub async fn send(&self, data: Bytes) -> Result<()> {
        if *self.closed.read().await {
            return Err(NethernetError::ConnectionClosed);
        }

        let channel = {
            let guard = self.reliable_channel.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| NethernetError::DataChannel("Reliable channel not set".to_string()))?
                .clone()
        };

        let segments = Message::split_into_segments(data)?;
        for segment in segments {
            let encoded = segment.encode();
            channel
                .send(&encoded)
                .await
                .map_err(|e| NethernetError::DataChannel(e.to_string()))?;
        }

        Ok(())
    }

    /// Receives a message
    ///
    /// Note: This method temporarily takes ownership of the receiver to avoid
    /// holding the Mutex guard across an await point. If another caller attempts
    /// to recv() while a recv() is already in progress, they will receive an error.
    pub async fn recv(&self) -> Result<Option<Bytes>> {
        if *self.closed.read().await {
            return Ok(None);
        }

        // Take the receiver out of the Option to avoid holding the lock across await
        let mut rx = {
            let mut guard = self.packet_rx.lock().await;
            guard.take().ok_or_else(|| {
                NethernetError::DataChannel("Receiver already in use by another caller".to_string())
            })?
        };

        // Receive outside the lock
        let result = rx.recv().await;

        // Put the receiver back
        *self.packet_rx.lock().await = Some(rx);

        Ok(result)
    }

    /// Closes the session
    pub async fn close(&self) -> Result<()> {
        if *self.closed.read().await {
            return Ok(());
        }

        *self.closed.write().await = true;

        // Acquire lock, clone the channel, drop the lock, then close
        let reliable = self.reliable_channel.lock().await.clone();
        if let Some(channel) = reliable {
            let _ = channel.close().await;
        }

        let unreliable = self.unreliable_channel.lock().await.clone();
        if let Some(channel) = unreliable {
            let _ = channel.close().await;
        }

        self.peer_connection
            .close()
            .await
            .map_err(NethernetError::WebRtc)?;

        Ok(())
    }

    /// Checks connection state
    pub fn connection_state(&self) -> RTCIceConnectionState {
        self.peer_connection.ice_connection_state()
    }

    /// Returns the peer connection
    pub fn peer_connection(&self) -> Arc<RTCPeerConnection> {
        self.peer_connection.clone()
    }

    /// Checks if the session is closed
    pub async fn is_closed(&self) -> bool {
        *self.closed.read().await
    }
}
