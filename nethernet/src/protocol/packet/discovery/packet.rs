//! Discovery packet trait and marshaling/unmarshaling utilities.

use super::crypto::{compute_checksum, decrypt, encrypt, verify_checksum};
use super::{MessagePacket, RequestPacket, ResponsePacket};
use crate::error::{NethernetError, Result};
use crate::protocol::constants::{ID_MESSAGE_PACKET, ID_REQUEST_PACKET, ID_RESPONSE_PACKET};
use crate::protocol::types::{U16LE, U64LE};
use std::io::{Cursor, Read, Write};

/// Trait for discovery packets used in LAN discovery.
pub trait Packet: Send + Sync {
    /// Returns the unique ID of the packet.
    fn id(&self) -> u16;

    /// Reads/decodes the packet data from the reader.
    fn read(&mut self, r: &mut dyn Read) -> Result<()>;

    /// Writes the packet data into the writer.
    fn write(&self, w: &mut dyn Write) -> Result<()>;

    /// Allows downcasting to concrete packet types.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Header of a discovery packet.
#[derive(Debug, Clone)]
pub struct Header {
    /// Packet ID
    pub packet_id: u16,
    /// Sender network ID
    pub sender_id: u64,
}

impl Header {
    /// Reads and decodes the header from the reader.
    pub fn read(r: &mut dyn Read) -> Result<Self> {
        let packet_id = U16LE::read(r)?.0;
        let sender_id = U64LE::read(r)?.0;

        // Discard 8-byte padding
        let mut padding = [0u8; 8];
        r.read_exact(&mut padding)?;

        Ok(Self {
            packet_id,
            sender_id,
        })
    }

    /// Writes the binary structure of the header into the writer.
    pub fn write(&self, w: &mut dyn Write) -> Result<()> {
        U16LE(self.packet_id).write(w)?;
        U64LE(self.sender_id).write(w)?;
        // 8-byte padding
        w.write_all(&[0u8; 8])?;
        Ok(())
    }
}

/// Marshals (encodes) a packet into bytes with the sender ID.
///
/// The packet structure is:
/// - HMAC-SHA256 checksum (32 bytes)
/// - AES-ECB encrypted payload:
///   - Length (uint16)
///   - Packet ID (uint16)
///   - Sender ID (uint64)
///   - Padding (8 bytes)
///   - Packet data
pub fn marshal(packet: &dyn Packet, sender_id: u64) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    // Write header
    let header = Header {
        packet_id: packet.id(),
        sender_id,
    };
    header.write(&mut buf)?;

    // Write packet data
    packet.write(&mut buf)?;

    // Prepend length
    // Validate that buf.len() fits in u16 to prevent silent truncation
    if buf.len() > u16::MAX as usize {
        return Err(NethernetError::MessageTooLarge(buf.len()));
    }
    let length = buf.len() as u16;
    let mut payload = Vec::new();
    U16LE(length).write(&mut payload)?;
    payload.extend_from_slice(&buf);

    // Encrypt the payload
    let encrypted = encrypt(&payload)?;

    // Compute HMAC-SHA256 checksum
    let checksum = compute_checksum(&payload);

    // Combine checksum + encrypted data
    let mut result = Vec::new();
    result.extend_from_slice(&checksum);
    result.extend_from_slice(&encrypted);

    Ok(result)
}

/// Unmarshals (decodes) a packet from bytes and returns it with the sender ID.
pub fn unmarshal(data: &[u8]) -> Result<(Box<dyn Packet>, u64)> {
    if data.len() < 32 {
        return Err(NethernetError::Other("packet too short".to_string()));
    }

    // Extract checksum and encrypted payload
    let checksum: [u8; 32] = data[..32].try_into().unwrap();
    let encrypted = &data[32..];

    // Decrypt the payload
    let payload = decrypt(encrypted)?;

    // Verify checksum
    if !verify_checksum(&payload, &checksum) {
        return Err(NethernetError::Other("checksum mismatch".to_string()));
    }

    let mut cursor = Cursor::new(payload);

    // Read length (2 bytes) - this was missing!
    let _length = U16LE::read(&mut cursor)?;

    // Read header
    let header = Header::read(&mut cursor)?;

    // Create appropriate packet based on ID
    let mut packet: Box<dyn Packet> = match header.packet_id {
        ID_REQUEST_PACKET => Box::new(RequestPacket::default()),
        ID_RESPONSE_PACKET => Box::new(ResponsePacket::default()),
        ID_MESSAGE_PACKET => Box::new(MessagePacket::default()),
        _ => {
            return Err(NethernetError::Other(format!(
                "unknown packet ID: {}",
                header.packet_id
            )));
        }
    };

    // Read packet data
    packet.read(&mut cursor)?;

    Ok((packet, header.sender_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = Header {
            packet_id: 0x01,
            sender_id: 0x1234567890abcdef,
        };

        let mut buf = Vec::new();
        header.write(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = Header::read(&mut cursor).unwrap();

        assert_eq!(header.packet_id, decoded.packet_id);
        assert_eq!(header.sender_id, decoded.sender_id);
    }
}
