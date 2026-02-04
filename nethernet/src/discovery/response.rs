//! Discovery response packet.
//!
//! Sent by servers in response to a RequestPacket from clients
//! to advertise the world/server information.

use std::io::{Read, Write};
use crate::error::Result;
use super::packet::{Packet, ID_RESPONSE_PACKET, read_bytes_u32, write_bytes_u32};

/// ResponsePacket is sent by servers to respond to discovery requests.
/// It contains hex-encoded ServerData payload.
#[derive(Debug, Clone, Default)]
pub struct ResponsePacket {
    /// Application-specific data (typically ServerData in Minecraft: Bedrock Edition)
    pub application_data: Vec<u8>,
}

impl ResponsePacket {
    /// Creates a new ResponsePacket with the given application data.
    pub fn new(application_data: Vec<u8>) -> Self {
        Self { application_data }
    }
}

impl Packet for ResponsePacket {
    fn id(&self) -> u16 {
        ID_RESPONSE_PACKET
    }
    
    fn read(&mut self, r: &mut dyn Read) -> Result<()> {
        // Read hex-encoded data
        let hex_data = read_bytes_u32(r)?;
        
        // Decode from hex
        let mut decoded = vec![0u8; hex_data.len() / 2];
        hex::decode_to_slice(&hex_data, &mut decoded)
            .map_err(|e| crate::error::NethernetError::Other(format!("hex decode error: {}", e)))?;
        
        self.application_data = decoded;
        Ok(())
    }
    
    fn write(&self, w: &mut dyn Write) -> Result<()> {
        // Encode to hex
        let hex_encoded = hex::encode(&self.application_data);
        write_bytes_u32(w, hex_encoded.as_bytes())?;
        Ok(())
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
