//! Tokio-based NetherNet protocol implementation.
//!
//! This crate provides high-level types for creating NetherNet clients and servers using WebRTC:
//! - [`NethernetStream`] for client connections
//! - [`NethernetListener`] for server-side connection acceptance
//! - [`Session`] for WebRTC peer connection management
//! - [`Signaling`] trait and implementations (LAN discovery)
//!
//! ## Features
//!
//! - WebRTC-based peer-to-peer connections
//! - ICE, DTLS, SCTP transport support
//! - Reliable and unreliable data channels
//! - Message segmentation (for messages over 10KB)
//! - LAN discovery with AES-ECB encryption and HMAC-SHA256 authentication
//! - Signal protocol compatible with Go implementation

pub mod error;
pub mod protocol;
pub mod session;
pub mod signaling;
pub mod transport;

pub use error::{NethernetError, Result};
pub use protocol::packet::discovery::{MessagePacket, RequestPacket, ResponsePacket, ServerData};
pub use protocol::{ConnectError, Message, MessageSegment, NegotiationMessage, Signal, SignalType};
pub use session::Session;
pub use signaling::{Notifier, Signaling};
pub use transport::{NethernetListener, NethernetStream};
