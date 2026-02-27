use std::net::SocketAddr;
use std::time::Duration;

use crate::transport::listener::RaknetListenerConfig;

/// Configuration builder for [`crate::transport::RaknetListener`].
#[derive(Debug, Clone)]
pub struct RaknetListenerConfigBuilder {
    bind_addr: Option<SocketAddr>,
    max_connections: usize,
    max_pending_connections: usize,
    max_mtu: u16,
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
    sent_datagram_timeout: Duration,
}

impl Default for RaknetListenerConfigBuilder {
    /// Create a RaknetListenerConfigBuilder populated with the default listener configuration values.
    ///
    /// The builder copies defaults from `RaknetListenerConfig::default()` but leaves `bind_addr` as
    /// `None` so callers must provide an explicit bind address before building.
    fn default() -> Self {
        let config = RaknetListenerConfig::default();
        Self {
            bind_addr: None,
            max_connections: config.max_connections,
            max_pending_connections: config.max_pending_connections,
            max_mtu: config.max_mtu,
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
            sent_datagram_timeout: config.sent_datagram_timeout,
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

    /// Set the maximum MTU (maximum transmission unit) that the listener will use.
    pub fn max_mtu(mut self, mtu: u16) -> Self {
        self.max_mtu = mtu;
        self
    }

    /// Sets the inactivity timeout used to detect and close idle sessions.
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

    /// Sets the sent datagram timeout.
    pub fn sent_datagram_timeout(mut self, timeout: Duration) -> Self {
        self.sent_datagram_timeout = timeout;
        self
    }

    /// Builds a `RaknetListenerConfig` from the builder.
    ///
    /// This consumes the builder and returns a concrete `RaknetListenerConfig`.
    /// Panics if a `bind_addr` was not supplied to the builder (missing config value).
    pub fn build(self) -> RaknetListenerConfig {
        let addr = self
            .bind_addr
            .ok_or_else(|| crate::RaknetError::MissingConfigValue("bind_addr".to_string()))
            .unwrap();
        RaknetListenerConfig {
            bind_addr: addr,
            max_connections: self.max_connections,
            max_pending_connections: self.max_pending_connections,
            max_mtu: self.max_mtu,
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
            sent_datagram_timeout: self.sent_datagram_timeout,
        }
    }
}
