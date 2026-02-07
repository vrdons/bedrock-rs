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
    fn id(&self) -> u16 {
        ID_REQUEST_PACKET
    }

    fn read(&mut self, _r: &mut dyn Read) -> Result<()> {
        // No data to read
        Ok(())
    }

    fn write(&self, _w: &mut dyn Write) -> Result<()> {
        // No data to write
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
