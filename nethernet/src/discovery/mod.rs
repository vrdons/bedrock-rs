//! LAN discovery module for NetherNet.
//!
//! This module provides encryption, packet handling, and server advertisement
//! for LAN discovery using UDP broadcast on port 7551.
//!
//! # Discovery Flow
//!
//! 1. Client broadcasts [`RequestPacket`] to network
//! 2. Server responds with [`ResponsePacket`] containing [`ServerData`]
//! 3. Both exchange [`MessagePacket`] for WebRTC negotiation
//!
//! All packets are encrypted with AES-ECB and authenticated with HMAC-SHA256.

mod crypto;
mod packet;
mod request;
mod response;
mod message;
mod server_data;

pub use packet::{Packet, Header, marshal, unmarshal};
pub use packet::{ID_REQUEST_PACKET, ID_RESPONSE_PACKET, ID_MESSAGE_PACKET};
pub use request::RequestPacket;
pub use response::ResponsePacket;
pub use message::MessagePacket;
pub use server_data::ServerData;

/// Default LAN discovery port
pub const LAN_DISCOVERY_PORT: u16 = 7551;
