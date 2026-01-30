//!
//!
//! ## Example: Client
//!
//! ```rust,no_run
//! use raknet::RaknetStream;
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut client = RaknetStream::connect("127.0.0.1:19132".parse()?).await?;
//!     client.send("Hello!").await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Example: Server
//!
//! ```rust,no_run
//! use raknet::RaknetListener;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut listener = RaknetListener::bind("0.0.0.0:19132".parse()?).await?;
//!     while let Some(mut conn) = listener.accept().await {
//!         tokio::spawn(async move {
//!             while let Some(msg) = conn.recv().await {
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
