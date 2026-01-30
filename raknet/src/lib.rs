//!
//!
//! ## Example: Client
//!
//! ```rust,no_run
//! use raknet::{RaknetStream, transport::RaknetStreamConfigBuilder};
//! use bytes::Bytes;
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let addr: SocketAddr = "127.0.0.1:19132".parse()?;
//!     let mut client = RaknetStream::connect(RaknetStreamConfigBuilder::new()
//!            .connect_addr(addr)
//!            .build()).await?;
//!     client.send("Hello!").await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Example: Server
//!
//! ```rust,no_run
//! use raknet::{RaknetListener, transport::RaknetListenerConfigBuilder};
//! use futures::StreamExt;
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let addr: SocketAddr = "0.0.0.0:19132".parse()?;
//!     let mut listener = RaknetListener::bind(RaknetListenerConfigBuilder::new()
//!         .bind_address(addr)
//!         .build()).await?;
//!
//!     // RaknetListener implements Stream
//!     while let Some(mut conn) = listener.next().await {
//!         tokio::spawn(async move {
//!             // RaknetStream also implements Stream
//!             while let Some(msg) = conn.next().await {
//!                 // Handle packet
//!             }
//!         });
//!     }
//!     Ok(())
//! }
//! ```
pub mod error;
pub mod protocol;
pub mod session;
pub mod transport;

pub use error::RaknetError;
pub use transport::{RaknetListener, RaknetStream};
