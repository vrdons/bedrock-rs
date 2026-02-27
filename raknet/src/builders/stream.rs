use std::net::SocketAddr;
use std::time::Duration;

use crate::transport::stream::RaknetStreamConfig;

/// Configuration builder for [`crate::transport::RaknetStream`].
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

impl From<RaknetStreamConfigBuilder> for RaknetStreamConfig {
    fn from(builder: RaknetStreamConfigBuilder) -> Self {
        builder.build()
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
