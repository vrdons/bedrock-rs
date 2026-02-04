//! Constants for the NetherNet discovery protocol.

/// Default UDP port used for LAN discovery.
/// Servers should listen for RequestPackets on this port.
pub const DEFAULT_PORT: u16 = 7551;

/// Application ID used to derive the encryption key.
/// Defined as 0xDEADBEEF.
pub const APPLICATION_ID: u64 = 0xDEADBEEF;

/// Packet IDs
pub const ID_REQUEST_PACKET: u16 = 0;
pub const ID_RESPONSE_PACKET: u16 = 1;
pub const ID_MESSAGE_PACKET: u16 = 2;

/// Maximum message size (in bytes)
pub const MAX_MESSAGE_SIZE: usize = 10000;

/// Packet header size (in bytes)
/// PacketID (2) + SenderID (8) + Padding (8) = 18 bytes
pub const HEADER_SIZE: usize = 18;


pub const RELIABLE_CHANNEL: &str = "ReliableDataChannel";
pub const UNRELIABLE_CHANNEL: &str = "UnreliableDataChannel";

/// Default capacity for the bounded packet channel in sessions.
/// This prevents unbounded memory growth while allowing for bursts of incoming packets.
pub const DEFAULT_PACKET_CHANNEL_CAPACITY: usize = 1024;
