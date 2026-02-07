use std::time::Instant;

use crate::protocol::{
    encapsulated_packet::EncapsulatedPacket,
    packet::{ConnectedPing, RaknetPacket},
    reliability::Reliability,
    state::{DisconnectReason, RakPriority},
};

use super::{ConnectionState, ManagedSession};

impl ManagedSession {
    /// Run periodic maintenance and return any datagrams that should be sent.
    pub fn on_tick(&mut self, now: Instant) -> Vec<crate::session::OutgoingDatagram> {
        if self.is_connected() {
            let idle = now.saturating_duration_since(self.last_activity);
            if idle >= self.config.session_timeout {
                let _ = self.send_disconnect(DisconnectReason::TimedOut);
                self.state = ConnectionState::Closed;
                self.last_disconnect_reason = Some(DisconnectReason::TimedOut);
            } else if idle >= self.config.session_stale {
                self.state = ConnectionState::Stale;
            }
        }

        if self.should_send_ping(now) {
            self.send_connected_ping(now);
        }

        self.enforce_queue_limit();

        self.inner.on_tick(now)
    }

    pub(crate) fn should_send_ping(&self, now: Instant) -> bool {
        if !self.is_connected() {
            return false;
        }
        if self.current_ping_nonce.is_some() {
            return false;
        }
        match self.last_ping_sent {
            Some(last) => now.saturating_duration_since(last) >= self.config.ping_interval,
            None => true,
        }
    }

    /// Queues a ConnectedPing control packet and records ping state using the provided time.
    ///
    /// The function computes the RakNet timestamp for `now`, enqueues a `ConnectedPing` packet
    /// with `Unreliable` reliability and `Immediate` priority, sets `last_ping_sent` to `now`,
    /// and stores the ping nonce derived from the timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::time::Instant;
    /// # // create a ManagedSession named `session` in a connected state for the example
    /// let mut session = /* ManagedSession::new_connected(...) */ todo!();
    /// let now = Instant::now();
    /// session.send_connected_ping(now);
    /// // after calling, session.last_ping_sent is Some(now) and current_ping_nonce is set
    /// ```
    pub(crate) fn send_connected_ping(&mut self, now: Instant) {
        let timestamp = Self::current_raknet_time(now);

        let pkt = RaknetPacket::ConnectedPing(ConnectedPing {
            ping_time: timestamp,
        });

        self.queue_control_packet(pkt, Reliability::Unreliable, 0, RakPriority::Immediate);
        self.last_ping_sent = Some(now);
        self.current_ping_nonce = Some(timestamp.0);
    }

    /// Enforces the configured maximum for queued reliable bytes and closes the session if exceeded.
    ///
    /// If a `max_queued_reliable_bytes` limit is configured and the current `queued_reliable_bytes`
    /// exceeds that limit, the session will be disconnected with reason `QueueTooLong`, the connection
    /// state will be set to `Closed`, and `last_disconnect_reason` will be recorded.
    ///
    /// # Examples
    ///
    /// ```
    /// // Construct a session whose queued_reliable_bytes exceed the configured limit,
    /// // then enforce the limit and observe the session is closed.
    /// let mut sess = ManagedSession {
    ///     config: Config { max_queued_reliable_bytes: Some(100), ..Default::default() },
    ///     queued_reliable_bytes: 200,
    ///     state: ConnectionState::Connected,
    ///     last_disconnect_reason: None,
    ///     ..Default::default()
    /// };
    /// sess.enforce_queue_limit();
    /// assert_eq!(sess.state, ConnectionState::Closed);
    /// assert_eq!(sess.last_disconnect_reason, Some(DisconnectReason::QueueTooLong));
    /// ```
    pub(crate) fn enforce_queue_limit(&mut self) {
        if let Some(limit) = self.config.max_queued_reliable_bytes {
            if self.queued_reliable_bytes > limit {
                let _ = self.send_disconnect(DisconnectReason::QueueTooLong);
                self.state = ConnectionState::Closed;
                self.last_disconnect_reason = Some(DisconnectReason::QueueTooLong);
            }
        }
    }

    pub(crate) fn debit_reliable_bytes(&mut self, packets: &[EncapsulatedPacket]) {
        for pkt in packets {
            if pkt.header.reliability.is_reliable() {
                self.queued_reliable_bytes = self.queued_reliable_bytes.saturating_sub(pkt.size());
            }
        }
    }
}