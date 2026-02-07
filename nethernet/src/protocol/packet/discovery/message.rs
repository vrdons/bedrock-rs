//! Discovery message packet.
//!
//! Sent by both server and client to negotiate a NetherNet connection
//! and exchange ICE candidates.

use super::packet::Packet;
use crate::error::Result;
use crate::protocol::constants::ID_MESSAGE_PACKET;
use crate::protocol::types::{U64LE, read_bytes_u32, write_bytes_u32};
use std::io::{Read, Write};

/// MessagePacket is used for negotiating WebRTC connections.
/// It contains the recipient network ID and signaling data.
#[derive(Debug, Clone, Default)]
pub struct MessagePacket {
    /// Network ID of the recipient (not the connection ID)
    pub recipient_id: u64,
    /// Signaling data (string form of Signal)
    pub data: String,
}

impl MessagePacket {
    /// Creates a new MessagePacket with the given recipient ID and data.
    pub fn new(recipient_id: u64, data: String) -> Self {
        Self { recipient_id, data }
    }
}

impl Packet for MessagePacket {
    fn id(&self) -> u16 {
        ID_MESSAGE_PACKET
    }

    fn read(&mut self, r: &mut dyn Read) -> Result<()> {
        self.recipient_id = U64LE::read(r)?.0;
        let data_bytes = read_bytes_u32(r)?;
        self.data = String::from_utf8(data_bytes)
            .map_err(|e| crate::error::NethernetError::Other(format!("invalid UTF-8: {}", e)))?;
        Ok(())
    }

    fn write(&self, w: &mut dyn Write) -> Result<()> {
        U64LE(self.recipient_id).write(w)?;
        write_bytes_u32(w, self.data.as_bytes())?;
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
