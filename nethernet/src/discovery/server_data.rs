//! ServerData binary structure for Minecraft: Bedrock Edition.
//!
//! Encapsulated in ResponsePacket.ApplicationData and sent in response
//! to RequestPacket broadcasted by clients on port 7551.

use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use crate::error::{NethernetError, Result};
use super::packet::{read_bytes_u8, write_bytes_u8};

/// Current version of ServerData supported by the discovery module.
const VERSION: u8 = 4;

/// ServerData defines the binary structure representing worlds in Minecraft: Bedrock Edition.
#[derive(Debug, Clone)]
pub struct ServerData {
    /// Name of the server (typically the player name of the owner)
    pub server_name: String,
    /// Name of the world/level
    pub level_name: String,
    /// Default game mode (0=Survival, 1=Creative, 2=Adventure, 3=Spectator)
    pub game_type: u8,
    /// Current player count (should be at least 1 to appear in server list)
    pub player_count: i32,
    /// Maximum player count allowed
    pub max_player_count: i32,
    /// Whether this is an Editor Mode project
    pub editor_world: bool,
    /// Whether hardcore mode is enabled
    pub hardcore: bool,
    /// Transport layer (2 = NetherNet)
    pub transport_layer: u8,
    /// Connection type (4 = LAN)
    pub connection_type: u8,
}

impl ServerData {
    /// Creates a new ServerData with default values.
    pub fn new(server_name: String, level_name: String) -> Self {
        Self {
            server_name,
            level_name,
            game_type: 0,
            player_count: 1,
            max_player_count: 8,
            editor_world: false,
            hardcore: false,
            transport_layer: 2, // NetherNet
            connection_type: 4, // LAN
        }
    }

    /// Encodes ServerData to binary format.
    pub fn marshal(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        
        // Write version
        buf.write_u8(VERSION)?;
        
        // Write server name (u8-prefixed string)
        write_bytes_u8(&mut buf, self.server_name.as_bytes())?;
        
        // Write level name (u8-prefixed string)
        write_bytes_u8(&mut buf, self.level_name.as_bytes())?;
        
        // Write game type (shifted left by 1)
        buf.write_u8(self.game_type << 1)?;
        
        // Write player counts
        buf.write_i32::<LittleEndian>(self.player_count)?;
        buf.write_i32::<LittleEndian>(self.max_player_count)?;
        
        // Write booleans
        buf.write_u8(if self.editor_world { 1 } else { 0 })?;
        buf.write_u8(if self.hardcore { 1 } else { 0 })?;
        
        // Write transport layer and connection type (both shifted left by 1)
        buf.write_u8(self.transport_layer << 1)?;
        buf.write_u8(self.connection_type << 1)?;
        
        Ok(buf)
    }

    /// Decodes ServerData from binary format.
    pub fn unmarshal(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        
        // Read and verify version
        let version = cursor.read_u8()?;
        if version != VERSION {
            return Err(NethernetError::Other(
                format!("version mismatch: got {}, want {}", version, VERSION)
            ));
        }
        
        // Read server name
        let server_name_bytes = read_bytes_u8(&mut cursor)?;
        let server_name = String::from_utf8(server_name_bytes)
            .map_err(|e| NethernetError::Other(format!("invalid server name UTF-8: {}", e)))?;
        
        // Read level name
        let level_name_bytes = read_bytes_u8(&mut cursor)?;
        let level_name = String::from_utf8(level_name_bytes)
            .map_err(|e| NethernetError::Other(format!("invalid level name UTF-8: {}", e)))?;
        
        // Read game type (shift right by 1)
        let game_type = cursor.read_u8()? >> 1;
        
        // Read player counts
        let player_count = cursor.read_i32::<LittleEndian>()?;
        let max_player_count = cursor.read_i32::<LittleEndian>()?;
        
        // Read booleans
        let editor_world = cursor.read_u8()? != 0;
        let hardcore = cursor.read_u8()? != 0;
        
        // Read transport layer and connection type (both shift right by 1)
        let transport_layer = cursor.read_u8()? >> 1;
        let connection_type = cursor.read_u8()? >> 1;
        
        // Ensure all data was read
        let remaining = data.len() - cursor.position() as usize;
        if remaining != 0 {
            return Err(NethernetError::Other(format!("unread {} bytes", remaining)));
        }
        
        Ok(Self {
            server_name,
            level_name,
            game_type,
            player_count,
            max_player_count,
            editor_world,
            hardcore,
            transport_layer,
            connection_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_data_roundtrip() {
        let original = ServerData {
            server_name: "Test Server".to_string(),
            level_name: "My World".to_string(),
            game_type: 0,
            player_count: 3,
            max_player_count: 10,
            editor_world: false,
            hardcore: false,
            transport_layer: 2,
            connection_type: 4,
        };

        let encoded = original.marshal().unwrap();
        let decoded = ServerData::unmarshal(&encoded).unwrap();

        assert_eq!(original.server_name, decoded.server_name);
        assert_eq!(original.level_name, decoded.level_name);
        assert_eq!(original.game_type, decoded.game_type);
        assert_eq!(original.player_count, decoded.player_count);
        assert_eq!(original.max_player_count, decoded.max_player_count);
        assert_eq!(original.editor_world, decoded.editor_world);
        assert_eq!(original.hardcore, decoded.hardcore);
        assert_eq!(original.transport_layer, decoded.transport_layer);
        assert_eq!(original.connection_type, decoded.connection_type);
    }

    #[test]
    fn test_version_mismatch() {
        let data = vec![5]; // Wrong version
        let result = ServerData::unmarshal(&data);
        assert!(result.is_err());
    }
}
