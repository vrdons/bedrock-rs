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
    /// Create a Session using the default packet channel capacity.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// // `pc` should be an initialized `RTCPeerConnection`
    /// let pc: Arc<webrtc::peer_connection::RTCPeerConnection> = Arc::new(/* ... */);
    /// let session = Session::new(pc);
    /// ```
    pub fn new(peer_connection: Arc<RTCPeerConnection>) -> Self {
        Self::with_capacity(peer_connection, DEFAULT_PACKET_CHANNEL_CAPACITY)
    }

    /// Creates a Session backed by the given RTCPeerConnection and a bounded packet channel with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// let pc = Arc::new(RTCPeerConnection::new());
    /// let session = Session::with_capacity(pc, 32);
    /// ```
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

    /// Attach a reliable RTCDataChannel to the session and route incoming message segments into the session's reassembly pipeline.
    ///
    /// The provided channel will receive an `on_message` handler that decodes incoming bytes as `MessageSegment`s, accumulates segments in the session's internal buffer, and forwards completed messages to the session's packet receiver. The channel is then stored as the session's reliable data channel.
    ///
    /// # Parameters
    ///
    /// - `channel` — an `Arc<RTCDataChannel>` to be used for reliable, ordered delivery and inbound message handling.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error variant of `NethernetError` if the operation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # async fn example(session: &nethernet::session::Session, channel: Arc<webrtc::data_channel::RTCDataChannel>) -> nethernet::Result<()> {
    /// session.set_reliable_channel(channel).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_reliable_channel(&self, channel: Arc<RTCDataChannel>) -> Result<()> {
        let message_buffer = self.message_buffer.clone();
        let packet_tx = self.packet_tx.clone();

        channel.on_message(Box::new(move |msg| {
            let data = msg.data.clone();
            let buffer = message_buffer.clone();
            let tx = packet_tx.clone();

            Box::pin(async move {
                let data_len = data.len();
                match MessageSegment::decode(data.as_ref()) {
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

    /// Attaches an unreliable RTC data channel to the session.
    ///
    /// Replaces any previously set unreliable data channel with the provided one.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use webrtc::data::data_channel::RTCDataChannel;
    ///
    /// async fn example(session: &Session, channel: Arc<RTCDataChannel>) {
    ///     session.set_unreliable_channel(channel).await.unwrap();
    /// }
    /// ```
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    pub async fn set_unreliable_channel(&self, channel: Arc<RTCDataChannel>) -> Result<()> {
        *self.unreliable_channel.lock().await = Some(channel);
        Ok(())
    }

    /// Send data over the session using the reliable data channel, splitting the payload into protocol segments as needed.
    ///
    /// # Errors
    ///
    /// - Returns `NethernetError::ConnectionClosed` if the session has been closed.
    /// - Returns `NethernetError::DataChannel(...)` if the reliable channel is not set or if sending a segment fails.
    /// - Returns any error produced by `Message::split_into_segments` when segmenting the input.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(session: &nethernet::session::Session) -> Result<(), Box<dyn std::error::Error>> {
    /// let data = bytes::Bytes::from("hello world");
    /// session.send(data).await?;
    /// # Ok(())
    /// # }
    /// ```
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

    /// Receive the next complete packet from the session.
    ///
    /// This returns the next reassembled message produced by the session's incoming
    /// segment stream. If the session has been closed, or the underlying packet
    /// channel has been closed, this returns `Ok(None)`.
    ///
    /// Errors:
    /// - Returns `Err(NethernetError::DataChannel(...))` if another caller is
    ///   already awaiting `recv()` (the receiver is temporarily taken to avoid
    ///   holding a lock across an await point).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use bytes::Bytes;
    /// # use nethernet::session::Session;
    /// # use webrtc::peer_connection::RTCPeerConnection;
    /// # async fn example(pc: Arc<RTCPeerConnection>, session: &Session) {
    /// if let Ok(Some(bytes)) = session.recv().await {
    ///     // handle bytes
    /// } else {
    ///     // session closed or channel closed
    /// }
    /// # }
    /// ```
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

    /// Shuts down the session by marking it closed and closing any attached data channels and the peer connection.
    ///
    /// After this call the session is considered closed; calling `close` again is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `Err(NethernetError::WebRtc)` if closing the underlying peer connection fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use nethernet::session::Session;
    /// # async fn example(session: Arc<Session>) {
    /// session.close().await.unwrap();
    /// # }
    /// ```
    pub async fn close(&self) -> Result<()> {
        let mut closed = self.closed.write().await;
        if *closed {
            return Ok(());
        }
        *closed = true;
        drop(closed);

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

    /// Return the current ICE connection state of the underlying peer connection.
    ///
    /// This reflects the latest RTCPeerConnection ICE state (e.g., `Connected`, `Disconnected`, `Failed`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // given a `session: Session`
    /// let state = session.connection_state();
    /// println!("ICE state: {:?}", state);
    /// ```
    pub fn connection_state(&self) -> RTCIceConnectionState {
        self.peer_connection.ice_connection_state()
    }

    /// Gets a clone of the session's RTCPeerConnection.
    ///
    /// # Returns
    ///
    /// An `Arc<RTCPeerConnection>` pointing to the session's peer connection.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use webrtc::peer_connection::RTCPeerConnection;
    /// # fn example(session: &crate::session::Session) {
    /// let pc = session.peer_connection();
    /// let _clone = Arc::clone(&pc);
    /// # }
    /// ```
    pub fn peer_connection(&self) -> Arc<RTCPeerConnection> {
        self.peer_connection.clone()
    }

    /// Reports whether the session has been closed.
    ///
    /// # Returns
    ///
    /// `true` if the session is closed, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(session: &nethernet::session::Session) {
    /// let closed = session.is_closed().await;
    /// println!("closed: {}", closed);
    /// # }
    /// ```
    pub async fn is_closed(&self) -> bool {
        *self.closed.read().await
    }
}
