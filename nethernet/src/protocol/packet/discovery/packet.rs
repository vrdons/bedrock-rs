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
    /// Reads a discovery packet header from the provided reader.
    ///
    /// This reads a 16-bit little-endian packet ID, a 64-bit little-endian sender ID,
    /// then consumes and discards 8 bytes of padding.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the reader does not contain enough bytes or an underlying
    /// read operation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    ///
    /// // Construct a buffer containing:
    /// // - packet_id: 1 (u16 little-endian)
    /// // - sender_id: 0x0102030405060708 (u64 little-endian)
    /// // - 8 bytes of zero padding
    /// let mut buf = Vec::new();
    /// buf.extend(&1u16.to_le_bytes());
    /// buf.extend(&0x0807060504030201u64.to_le_bytes());
    /// buf.extend(&[0u8; 8]);
    ///
    /// let mut cur = Cursor::new(buf);
    /// let hdr = Header::read(&mut cur).unwrap();
    /// assert_eq!(hdr.packet_id, 1);
    /// assert_eq!(hdr.sender_id, 0x0807060504030201u64);
    /// ```
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

    /// Serialize the header into the provided writer using little-endian encoding and fixed padding.
    ///
    /// Writes the header fields in order:
    /// 1. `packet_id` as a 16-bit little-endian integer.
    /// 2. `sender_id` as a 64-bit little-endian integer.
    /// 3. Eight zero bytes of padding.
    ///
    /// # Parameters
    ///
    /// - `w`: destination writer that receives the serialized header bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any underlying write to `w` fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let hdr = Header { packet_id: 0x1234, sender_id: 0x1122334455667788 };
    /// let mut buf = Vec::new();
    /// hdr.write(&mut buf).unwrap();
    /// // 2 bytes (u16) + 8 bytes (u64) + 8 bytes padding = 18 bytes
    /// assert_eq!(buf.len(), 18);
    /// assert_eq!(&buf[0..2], &0x1234u16.to_le_bytes());
    /// assert_eq!(&buf[2..10], &0x1122334455667788u64.to_le_bytes());
    /// ```
    pub fn write(&self, w: &mut dyn Write) -> Result<()> {
        U16LE(self.packet_id).write(w)?;
        U64LE(self.sender_id).write(w)?;
        // 8-byte padding
        w.write_all(&[0u8; 8])?;
        Ok(())
    }
}

/// Encodes a discovery packet together with a sender ID into the wire format.
///
/// The output is: a 32-byte HMAC-SHA256 checksum followed by the AES-ECB encrypted payload.
/// The encrypted payload contains a 16-bit length prefix, the packet header (packet ID and sender ID),
/// 8 bytes of padding, and the packet-specific data.
/// Returns an error if the encoded packet exceeds 65,535 bytes or if any underlying write/encryption step fails.
///
/// # Examples
///
/// ```
/// struct Dummy;
/// impl Packet for Dummy {
///     fn id(&self) -> u16 { 0x02 }
///     fn read(&mut self, _r: &mut dyn std::io::Read) -> Result<(), NethernetError> { Ok(()) }
///     fn write(&self, _w: &mut dyn std::io::Write) -> Result<(), NethernetError> { Ok(()) }
///     fn as_any(&self) -> &dyn std::any::Any { self }
/// }
///
/// let packet = Dummy;
/// let bytes = marshal(&packet, 0x1234_5678_90ab_cdef).expect("marshal failed");
/// // Result must contain at least the 32-byte checksum
/// assert!(bytes.len() >= 32);
/// ```
///
/// # Returns
///
/// A Vec<u8> containing the serialized packet: the 32-byte HMAC-SHA256 checksum followed by the AES-ECB encrypted payload.
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

/// Decodes and verifies a discovery packet from raw bytes, returning the parsed packet and its sender ID.
///
/// The function expects the input to be a checksum (32 bytes) followed by an AES-ECB encrypted payload. It
/// decrypts the payload, verifies the HMAC-SHA256 checksum against the plaintext, reads the payload length and
/// header, instantiates the concrete packet type based on the header's packet ID, and delegates parsing of the
/// packet-specific fields to that packet's `read` implementation. Errors are returned for malformed data,
/// checksum mismatches, oversized/unknown packet IDs, or trailing bytes after parsing.
///
/// # Returns
///
/// A tuple containing the boxed concrete packet and the sender's 64-bit network ID.
///
/// # Examples
///
/// ```rust,no_run
/// // Example demonstrates call site; real packets must be produced by `marshal`.
/// let data: &[u8] = &[0u8; 32]; // too short / invalid — unmarshal should return an error
/// let result = unmarshal(data);
/// assert!(result.is_err());
/// ```
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
        ID_REQUEST_PACKET => Box::new(RequestPacket),
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

    // Validate that cursor has been fully consumed
    let cursor_position = cursor.position() as usize;
    let payload_len = cursor.get_ref().len();
    if cursor_position < payload_len {
        let remaining = payload_len - cursor_position;
        return Err(NethernetError::Other(format!(
            "trailing data in packet: {} remaining bytes out of {} total payload bytes",
            remaining, payload_len
        )));
    }

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