use std::time::Instant;

use super::{OutgoingDatagram, Session};

impl Session {
    /// Periodic maintenance: prune splits, schedule resends, and emit ACK/NACK datagrams.
    pub fn on_tick(&mut self, now: Instant) -> Vec<OutgoingDatagram> {
        let mut out = Vec::new();

        self.process_incoming_acks_naks(now);

        let dropped = self.split_assembler.prune(now);
        for (ch, idx) in dropped {
            if let (Some(ch), Some(idx)) = (ch, idx) {
                self.ordering.skip_index(ch, idx);
            }
        }

        let mut bw = self.sliding.get_retransmission_bandwidth();
        if bw > 0 {
            let to_resend = self.collect_resendable_datagrams(now, &mut bw);
            self.resend_datagrams(to_resend, now, &mut out);
        }

        if let Some(d) = self.build_ack_datagram(now) {
            out.push(d);
        }

        if let Some(d) = self.build_nak_datagram() {
            out.push(d);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{constants::DatagramFlags, datagram::DatagramPayload, types::Sequence24};

    use super::*;
    use std::time::Instant;

    #[test]
    fn ack_and_nak_tick_match_expected_ranges() {
        let mut s = Session::new(1200);
        let now = Instant::now();

        // Receive sequence 0 and 2, leaving a gap at 1.
        s.process_datagram_sequence(Sequence24::new(0));
        s.process_datagram_sequence(Sequence24::new(2));

        let out = s.on_tick(now);
        assert_eq!(out.len(), 2);

        let mut ack = None;
        let mut nak = None;
        for d in &out {
            let d_ref = match d {
                OutgoingDatagram::Shared(d) => d.as_ref(),
                OutgoingDatagram::Owned(d) => d,
            };
            if d_ref.header.flags.contains(DatagramFlags::ACK) {
                ack = Some(d_ref);
            }
            if d_ref.header.flags.contains(DatagramFlags::NACK) {
                nak = Some(d_ref);
            }
        }

        let ack = ack.expect("ack datagram");
        let nak = nak.expect("nak datagram");

        if let DatagramPayload::Ack(payload) = &ack.payload {
            assert_eq!(payload.ranges.len(), 2);
            assert_eq!(payload.ranges[0].start.value(), 0);
            assert_eq!(payload.ranges[0].end.value(), 0);
            assert_eq!(payload.ranges[1].start.value(), 2);
            assert_eq!(payload.ranges[1].end.value(), 2);
        } else {
            panic!("expected ack payload");
        }

        if let DatagramPayload::Nak(payload) = &nak.payload {
            assert_eq!(payload.ranges.len(), 1);
            assert_eq!(payload.ranges[0].start.value(), 1);
            assert_eq!(payload.ranges[0].end.value(), 1);
        } else {
            panic!("expected nak payload");
        }
    }

    #[test]
    fn clean_sent_datagrams_evicts_excess() {
        use crate::protocol::packet::RaknetPacket;
        use crate::session::SessionTunables;
        use std::time::Duration;
        let tunables = SessionTunables {
            max_sent_datagrams: 3,
            sent_datagram_timeout: Duration::from_secs(10),
            ..Default::default()
        };
        let mut session = Session::with_tunables(1200, tunables);
        let now = Instant::now();

        // Create 5 separate datagrams
        for i in 0..5 {
            session.queue_packet(
                RaknetPacket::UserData {
                    id: 0xFE,
                    payload: bytes::Bytes::from(vec![i as u8]),
                },
                crate::protocol::reliability::Reliability::Reliable,
                0,
                crate::protocol::state::RakPriority::Normal,
            );
            // Build datagram to track it.
            // Since we queue one small packet and immediately build, it should create one datagram per packet.
            let d = session.build_data_datagram(now);
            assert!(d.is_some(), "should build datagram {}", i);
        }

        // Initially, sent_datagrams should have 5 entries (before cleanup)
        assert_eq!(session.sent_datagrams.len(), 5);

        // Run tick, which triggers clean_sent_datagrams via process_incoming_acks_naks
        session.on_tick(now);

        // Should be trimmed to 3
        assert_eq!(session.sent_datagrams.len(), 3);

        // Base should have advanced by 2 (0 and 1 dropped). Base starts at 0.
        // Dropped: 0, 1. Remaining: 2, 3, 4.
        // Base should be 2.
        assert_eq!(session.sent_datagrams_base.value(), 2);
    }
}
