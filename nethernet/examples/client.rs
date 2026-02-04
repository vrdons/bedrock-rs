//! NetherNet client example using LAN discovery.
//!
//! This example demonstrates how to create a NetherNet client that:
//! - Discovers servers on LAN via broadcast
//! - Connects via WebRTC
//! - Sends and receives packets

use nethernet::NethernetStream;
use nethernet::signaling::lan::LanSignaling;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::Level;
use tracing_subscriber::{filter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fmt_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    let filter_layer = filter::LevelFilter::from_level(Level::DEBUG);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(filter_layer)
        .init();

    // Create client signaling with unique network ID
    // Use port 0 to let OS assign a free port (different from server's 7551)
    let network_id = rand::random::<u64>();
    let bind_addr: SocketAddr = "0.0.0.0:0".parse()?;
    
    let signaling = Arc::new(LanSignaling::new(network_id, bind_addr).await?);
    
    tracing::info!("NetherNet client starting");
    tracing::info!("   Network ID: {}", network_id);
    tracing::info!("   Scanning for servers on LAN...");
    
    // Wait for server discovery via broadcast
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Get discovered servers
    let servers = signaling.discover().await?;
    
    if servers.is_empty() {
        tracing::error!("No servers found on LAN!");
        tracing::info!("Make sure a server is running on port 7551");
        return Ok(());
    }
    
    // Select the first discovered server
    let (server_network_id, server_data) = servers.iter().next().unwrap();
    tracing::info!("🎯 Found server with network ID: {}", server_network_id);
    tracing::debug!("   Server data: {:?}", server_data);
    
    // Get server address from our address mapping
    // The server should be on the same network, so we can use any local interface
    let server_addr: SocketAddr = "0.0.0.0:7551".parse()?;
    
    tracing::info!("🔗 Connecting to server (network ID: {})", server_network_id);
    
    // Connect to the server using discovered network ID
    let stream = NethernetStream::connect(
        signaling.clone(),
        server_network_id.to_string(),
        server_addr,
    ).await?;
    
    tracing::info!("✅ Connected to server");
    
    // Send some test packets
    for i in 1..=10 {
        let message = format!("Hello from client, packet #{}", i);
        let data = Bytes::from(message.clone());
        
        tracing::debug!("📤 Sending: {}", message);
        stream.send(data).await?;
        
        // Receive echo response
        match stream.recv().await? {
            Some(response) => {
                let text = String::from_utf8_lossy(&response);
                tracing::debug!("📥 Received: {}", text);
            }
            None => {
                tracing::warn!("Connection closed by server");
                break;
            }
        }
        
        // Wait a bit between packets
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Test large packet (will be segmented)
    tracing::info!("📦 Sending large packet (20KB)...");
    let large_data = Bytes::from(vec![0xAB; 20_000]);
    stream.send(large_data).await?;
    
    if let Some(response) = stream.recv().await? {
        tracing::info!("✅ Large packet echoed back ({} bytes)", response.len());
    }
    
    // Close connection gracefully
    tracing::info!("👋 Closing connection...");
    stream.close().await?;
    tracing::info!("✅ Connection closed");
    
    Ok(())
}
