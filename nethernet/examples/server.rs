//! NetherNet server example using LAN discovery.
//!
//! This example demonstrates how to create a NetherNet server that:
//! - Broadcasts server information on LAN
//! - Accepts incoming WebRTC connections
//! - Handles packets from clients

use futures::StreamExt;
use nethernet::signaling::lan::LanSignaling;
use nethernet::{NethernetListener, ServerData, Signaling};
use std::net::SocketAddr;
use tracing::Level;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt};

/// Starts a NetherNet LAN-discoverable server that listens on 0.0.0.0:7551, responds to LAN discovery,
/// accepts incoming WebRTC client connections, and echoes received packets back to each client.
///
/// The server configures TRACE-level tracing to stdout, advertises a randomly generated network ID
/// and provided server metadata via LAN signaling, and spawns a dedicated task per client to handle
/// that client's packet loop.
///
/// # Returns
///
/// Returns `Ok(())` on normal shutdown. Returns an error if initialization (tracing, signaling,
/// listener binding) or I/O operations fail.
///
/// # Examples
///
/// ```no_run
/// // Run the example server (typically executed as a binary or example)
/// // cargo run --example server
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with environment filter
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    let filter_layer = filter::LevelFilter::from_level(Level::TRACE);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(filter_layer)
        .init();

    // Create server data for LAN discovery
    let server_data = ServerData::new(
        "My NetherNet Server".to_string(),
        "Example World".to_string(),
    );

    // Set up LAN signaling with unique network ID
    // IMPORTANT: Server must bind to 7551 to receive broadcast discovery requests
    let network_id = rand::random::<u64>();
    let bind_addr: SocketAddr = "0.0.0.0:7551".parse()?;

    let signaling = LanSignaling::new(network_id, bind_addr).await?;

    // Set server data for discovery responses
    signaling.set_pong_data(server_data.marshal()?);

    tracing::info!("NetherNet server starting");
    tracing::info!("   Network ID: {}", network_id);
    tracing::info!("   Listening on: {}", bind_addr);
    tracing::info!("   Broadcasting discovery responses...");

    // Create listener
    let mut listener = NethernetListener::bind(signaling, bind_addr).await?;
    tracing::info!("✅ Server ready and responding to LAN discovery");

    // Accept incoming connections
    while let Some(session) = listener.next().await {
        tracing::info!("🔗 New client connected");

        // Spawn a task to handle this client
        tokio::spawn(async move {
            let mut packet_count = 0;

            loop {
                match session.recv().await {
                    Ok(Some(data)) => {
                        packet_count += 1;
                        // Echo the packet back
                        if let Err(e) = session.send(data).await {
                            tracing::error!("Failed to send packet: {}", e);
                            break;
                        }
                    }
                    Ok(None) => {
                        tracing::info!("Client disconnected gracefully");
                        break;
                    }
                    Err(e) => {
                        tracing::error!("Error receiving packet: {}", e);
                        break;
                    }
                }
            }

            tracing::info!("Client session ended ({} packets received)", packet_count);
        });
    }

    Ok(())
}