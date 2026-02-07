//! Discovery request packet.
//!
//! Sent by clients to discover servers on the same network using the
//! broadcast address on port 7551.

use super::packet::Packet;
use crate::error::Result;
use crate::protocol::constants::ID_REQUEST_PACKET;
use std::io::{Read, Write};

/// RequestPacket is sent by clients to discover servers on LAN.
/// It does not contain any additional data beyond the header.
#[derive(Debug, Clone, Default)]
pub struct RequestPacket;

impl Packet for RequestPacket {
    /// Get the packet identifier for a discovery request.
    ///
    /// # Returns
    ///
    /// The numeric protocol identifier for a discovery request.
    ///
    /// # Examples
    ///
    /// ```
    /// let pkt = RequestPacket::default();
    /// assert_eq!(pkt.id(), ID_REQUEST_PACKET);
    /// ```
    fn id(&self) -> u16 {
        ID_REQUEST_PACKET
    }

    /// Reads the packet payload from the provided reader; no-op for a RequestPacket since it has no payload.
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::empty;
    /// let mut pkt = RequestPacket::default();
    /// let mut rdr = empty();
    /// assert!(pkt.read(&mut rdr).is_ok());
    /// ```
    fn read(&mut self, _r: &mut dyn Read) -> Result<()> {
        // No data to read
        Ok(())
    }

    /// Writes the packet to the provided writer; this packet has no payload so nothing is written.
    ///
    /// # Examples
    ///
    /// ```
    /// let pkt = RequestPacket::default();
    /// let mut buf: Vec<u8> = Vec::new();
    /// pkt.write(&mut buf).unwrap();
    /// assert!(buf.is_empty());
    /// ```
    ///
    /// # Returns
    ///
    /// `Ok(())` on success.
    fn write(&self, _w: &mut dyn Write) -> Result<()> {
        // No data to write
        Ok(())
    }

    /// Exposes the packet instance as a dynamically-typed `Any` to allow downcasting.
    ///
    /// This is used when the concrete packet type must be recovered from a trait object.
    ///
    /// # Returns
    ///
    /// A reference to `self` as `dyn std::any::Any`.
    ///
    /// # Examples
    ///
    /// ```
    /// let pkt = RequestPacket::default();
    /// let any_ref = pkt.as_any();
    /// assert!(any_ref.downcast_ref::<RequestPacket>().is_some());
    /// ```
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}