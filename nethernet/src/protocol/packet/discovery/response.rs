//! Discovery response packet.
//!
//! Sent by servers in response to a RequestPacket from clients
//! to advertise the world/server information.
use super::packet::Packet;
use crate::error::Result;
use crate::protocol::constants::ID_RESPONSE_PACKET;
use crate::protocol::types::{read_bytes_u32, write_bytes_u32};
use std::io::{Read, Write};

/// ResponsePacket is sent by servers to respond to discovery requests.
/// It contains hex-encoded ServerData payload.
#[derive(Debug, Clone, Default)]
pub struct ResponsePacket {
    /// Application-specific data (typically ServerData in Minecraft: Bedrock Edition)
    pub application_data: Vec<u8>,
}

impl ResponsePacket {
    /// Create a ResponsePacket containing the provided application data.
    ///
    /// # Examples
    ///
    /// ```
    /// let data = vec![0x01, 0x02, 0x03];
    /// let pkt = ResponsePacket::new(data.clone());
    /// assert_eq!(pkt.application_data, data);
    /// ```
    pub fn new(application_data: Vec<u8>) -> Self {
        Self { application_data }
    }
}

impl Packet for ResponsePacket {
    /// Provides the packet identifier for a discovery response packet.
    ///
    /// # Returns
    ///
    /// The numeric packet identifier (`ID_RESPONSE_PACKET`).
    fn id(&self) -> u16 {
        ID_RESPONSE_PACKET
    }

    /// Read hex-encoded application data from `r` and store the decoded bytes in `self.application_data`.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the encoded bytes fails, or if the hex decoding fails. Hex decode errors are converted to `crate::error::NethernetError::Other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    ///
    /// // prepare a buffer containing the hex-encoded payload written with the same length-prefix format
    /// let mut buf = Vec::new();
    /// let hex = hex::encode(b"hello");
    /// crate::protocol::types::write_bytes_u32(&mut buf, hex.as_bytes()).unwrap();
    ///
    /// let mut cursor = Cursor::new(buf);
    /// let mut pkt = crate::protocol::packet::discovery::response::ResponsePacket::default();
    /// pkt.read(&mut cursor).unwrap();
    /// assert_eq!(pkt.application_data, b"hello");
    /// ```
    fn read(&mut self, r: &mut dyn Read) -> Result<()> {
        // Read hex-encoded data
        let hex_data = read_bytes_u32(r)?;

        // Decode from hex
        self.application_data = hex::decode(&hex_data)
            .map_err(|e| crate::error::NethernetError::Other(format!("hex decode error: {}", e)))?;

        Ok(())
    }

    /// Writes the packet's application_data as a hex-encoded byte sequence (prefixed with a 32-bit length) to the provided writer.
    ///
    /// # Examples
    ///
    /// ```
    /// let pkt = ResponsePacket::new(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    /// let mut buf = Vec::new();
    /// pkt.write(&mut buf).unwrap();
    /// let hex = hex::encode(&[0xDE, 0xAD, 0xBE, 0xEF]);
    /// assert!(buf.ends_with(hex.as_bytes()));
    /// ```
    fn write(&self, w: &mut dyn Write) -> Result<()> {
        // Encode to hex
        let hex_encoded = hex::encode(&self.application_data);
        write_bytes_u32(w, hex_encoded.as_bytes())?;
        Ok(())
    }

    /// Exposes the receiver as a `dyn Any` so callers can perform runtime downcasting.
    ///
    /// # Examples
    ///
    /// ```
    /// let pkt = ResponsePacket::default();
    /// let any = pkt.as_any();
    /// let downcast = any.downcast_ref::<ResponsePacket>().unwrap();
    /// assert_eq!(downcast.application_data, pkt.application_data);
    /// ```
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
