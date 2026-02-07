use std::time::Instant;

use crate::protocol::{
    encapsulated_packet::EncapsulatedPacket,
    packet::{DecodeError, RaknetPacket},
    types::Sequence24,
};
use bytes::Bytes;

use crate::protocol::ack::{AckNackPayload, SequenceRange};

use super::{IncomingPacket, Session};

impl Session {
    /// Handle an incoming data payload (a list of encapsulated packets).
    pub fn handle_data_payload(
        &mut self,
        packets: Vec<EncapsulatedPacket>,
        now: Instant,
    ) -> Result<Vec<IncomingPacket>, DecodeError> {
        let mut out = Vec::new();

        self.sliding.on_packet_received(now);

        for enc in packets.into_iter() {
            self.handle_encapsulated(enc, now, &mut out)?;
        }

        Ok(out)
    }

    /// Handle an incoming dedicated ACK payload.
    pub fn handle_ack_payload(&mut self, payload: AckNackPayload) {
        self.incoming_acks.extend(payload.ranges);
    }

    /// Handle an incoming dedicated NACK payload.
    pub fn handle_nack_payload(&mut self, payload: AckNackPayload) {
        self.incoming_naks.extend(payload.ranges);
    }

    /// Processes a single encapsulated packet: performs deduplication for non-split reliable packets, hands split parts to the assembler, marks reliable indexes when a non-split packet is committed, and dispatches a reassembled or non-split packet for ordered handling or decoding.
    ///
    /// If the packet is a duplicate non-split reliable packet, it is dropped silently. Split parts are buffered until the assembler returns a full reassembled packet; reliable indexes for split parts are not marked until the full split is assembled. Decoder-produced ACK/NACK payloads are handled by the session as part of the decoding step.
    ///
    /// Parameters:
    /// - `enc`: the incoming encapsulated packet to process.
    /// - `now`: the current time used for split assembly bookkeeping and timeouts.
    /// - `out`: a mutable output vector that will receive any produced `IncomingPacket` instances.
    ///
    /// # Returns
    /// `Ok(())` on successful processing; `Err(DecodeError)` if the split assembler fails to accept the packet (for example, due to buffer capacity or decoding issues).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # no_run
    /// let mut session = Session::new();
    /// let enc = EncapsulatedPacket::from_bytes(&[ /* ... */ ]);
    /// let mut out = Vec::new();
    /// session.handle_encapsulated(enc, std::time::Instant::now(), &mut out).unwrap();
    /// ```
    fn handle_encapsulated(
        &mut self,
        enc: EncapsulatedPacket,
        now: Instant,
        out: &mut Vec<IncomingPacket>,
    ) -> Result<(), DecodeError> {
        // Reliability Logic:
        // - For non-split reliable packets:
        //   1) Deduplicate by reliable index; drop duplicates early.
        //   2) Decode/deliver; mark reliable index as seen on success.
        //
        // - For split packets:
        //   Do NOT mark reliable-indexes as seen until the split is fully assembled.
        //   Each split part has its own reliable index, and marking parts as seen
        //   would prevent retransmission if we later drop the split due to timeout.
        //   We rely on split_assembler to filter duplicate parts per (id,index).

        let is_split = enc.header.is_split;
        let ridx = if enc.header.reliability.is_reliable() && !is_split {
            enc.reliable_index
        } else {
            None
        };

        if let Some(idx) = ridx {
            if self.reliable_tracker.has_seen(idx) {
                // Duplicate non-split reliable; drop silently.
                return Ok(());
            }
        }

        // Attempt to add to split assembler (or pass through if not split)
        // Note: add() consumes the packet.
        let assembled_opt = match self.split_assembler.add(enc, now) {
            Ok(v) => v,
            Err(e) => {
                // If buffer is full, we return Error.
                // We have NOT marked the reliable index as seen.
                // Sender will timeout and retransmit. Ideally buffer clears by then.
                return Err(e);
            }
        };

        // If we reached here, the packet was either buffered or reassembled successfully.
        // For non-split reliable packets, commit the reliable index now.
        // For split packets, we avoid marking per-part indexes as seen; duplicates
        // are handled by split_assembler itself.
        if !is_split {
            if let Some(idx) = ridx {
                self.reliable_tracker.see(idx);
            }
        }

        let enc = match assembled_opt {
            Some(pkt) => pkt,
            None => return Ok(()), // Buffered partial split
        };

        if enc.header.reliability.is_ordered() {
            self.handle_ordered(enc, out)?;
        } else {
            self.decode_and_push(enc, out)?;
        }

        Ok(())
    }

    pub(crate) fn decode_and_push(
        &mut self,
        enc: EncapsulatedPacket,
        out: &mut Vec<IncomingPacket>,
    ) -> Result<(), DecodeError> {
        let mut buf = enc.payload.clone();
        let reliability = enc.header.reliability;
        let ordering_channel = enc.ordering_channel;

        let pkt = match RaknetPacket::decode(&mut buf) {
            Ok(pkt) => pkt,
            Err(DecodeError::UnknownId(id)) => {
                let body = if !enc.payload.is_empty() {
                    enc.payload.slice(1..)
                } else {
                    Bytes::new()
                };
                RaknetPacket::UserData { id, payload: body }
            }
            Err(e) => return Err(e),
        };

        if let RaknetPacket::EncapsulatedAck(payload) = pkt {
            self.incoming_acks.extend(payload.0.ranges);
            return Ok(());
        }
        if let RaknetPacket::EncapsulatedNak(payload) = pkt {
            self.incoming_naks.extend(payload.0.ranges);
            return Ok(());
        }

        out.push(IncomingPacket {
            packet: pkt,
            reliability,
            ordering_channel,
        });
        Ok(())
    }

    pub(crate) fn process_incoming_acks_naks(&mut self, now: Instant) {
        self.process_incoming_acks(now);
        self.process_incoming_naks(now);
    }

    /// Apply all queued incoming ACK ranges to tracked sent datagrams.
    ///
    /// For each acknowledged sequence in the queued ranges, removes the corresponding
    /// tracked datagram from `sent_datagrams`. If the removed datagram carries an
    /// `EncapsulatedPackets` payload, informs the sliding-window mechanism via
    /// `on_ack` using the provided `now`, the datagram, the sequence number, and
    /// the datagram's original send time.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Assume `session` is a mutable `Session` and `now` is an `Instant`.
    /// // This will process any ACK ranges previously pushed into `session`.
    /// session.process_incoming_acks(now);
    /// ```
    fn process_incoming_acks(&mut self, now: Instant) {
        while let Some(range) = self.incoming_acks.pop_front() {
            Self::for_each_sequence_in_range(range, |seq| {
                if let Some(tracked) = self.sent_datagrams.remove(&seq) {
                    if let crate::protocol::datagram::DatagramPayload::EncapsulatedPackets(_) =
                        &tracked.datagram.payload
                    {
                        self.sliding
                            .on_ack(now, &tracked.datagram, seq, tracked.send_time);
                    }
                }
            });
        }
    }

    /// Applies queued NAK ranges to sent datagrams and schedules retransmission.
    ///
    /// For each sequence number in the stored NAK ranges, if a tracked sent datagram exists
    /// and contains encapsulated packets, this marks a NAK with the sliding-window logic
    /// and resets that datagram's next-send time to `now` so it will be retransmitted.
    ///
    /// # Arguments
    ///
    /// * `now` - The instant to assign to `next_send` for retransmission scheduling.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Instant;
    /// // `session` must be an initialized Session with `incoming_naks` and `sent_datagrams` populated.
    /// // let mut session = /* obtain or construct Session */ ;
    /// // session.process_incoming_naks(Instant::now());
    /// ```
    fn process_incoming_naks(&mut self, now: Instant) {
        while let Some(range) = self.incoming_naks.pop_front() {
            Self::for_each_sequence_in_range(range, |seq| {
                if let Some(mut tracked) = self.sent_datagrams.remove(&seq) {
                    if let crate::protocol::datagram::DatagramPayload::EncapsulatedPackets(_) =
                        &tracked.datagram.payload
                    {
                        self.sliding.on_nak();
                        tracked.next_send = now;
                        self.sent_datagrams.insert(seq, tracked);
                    }
                }
            });
        }
    }

    fn for_each_sequence_in_range<F>(range: SequenceRange, mut f: F)
    where
        F: FnMut(Sequence24),
    {
        let mut seq = range.start;
        loop {
            f(seq);
            if seq == range.end {
                break;
            }
            seq = seq.next();
        }
    }
}
