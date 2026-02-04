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
//! - Full tokio and futures usage (like raknet)
//! - WebRTC-based peer-to-peer connections
//! - ICE, DTLS, SCTP transport support
//! - Reliable and unreliable data channels
//! - Message segmentation (for messages over 10KB)
//! - LAN discovery with AES-ECB encryption and HMAC-SHA256 authentication
//! - Xbox Live signaling via WebSocket
//! - Signal protocol compatible with Go implementation

pub mod discovery;
pub mod error;
pub mod protocol;
pub mod signaling;
pub mod session;
pub mod transport;

pub use discovery::{ServerData, RequestPacket, ResponsePacket, MessagePacket};
pub use error::{NethernetError, Result};
pub use protocol::{Signal, SignalType, Message, ConnectError, NegotiationMessage};
pub use protocol::{SegmentedMessage, SegmentAssembler, segment_packet};
pub use session::{Session};
pub use signaling::{Signaling, Notifier};
pub use transport::{NethernetListener, NethernetStream};
