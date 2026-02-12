# Bedrock Protocol Suite

This workspace contains high-performance Rust implementations for the Minecraft Bedrock Edition network protocol. It consists of two main components:

- **RakNet**: A reliable UDP-based communication protocol.
- **NetherNet**: The next-generation discovery and connection protocol based on WebRTC.

## Components

### 🚀 RakNet

The fundamental networking layer for Minecraft Bedrock Edition servers. This implementation is derived from [iAldrich23xX/tokio-raknet](https://github.com/iAldrich23xX/tokio-raknet) and has been updated with performance improvements and modern Rust practices.

- **Features:**
  - Asynchronous I/O (based on `tokio`)
  - Reliable/Unreliable packet transmission
  - Fragmented packet support
  - Flow control and congestion management

### 🌐 NetherNet

The WebRTC-based network protocol used in newer versions of Minecraft. It provides LAN discovery and secure peer-to-peer (P2P) connectivity.

- **Features:**
  - Secure communication over WebRTC (DTLS/SRTP)
  - LAN server discovery
  - Signaling management
  - Easy-to-use `NethernetListener` and `NethernetStream`

## Usage

To build the project:

```bash
cargo build --release
```

To run the examples:

```bash
# RakNet example (Forwarder)
cargo run --example forwarder -p raknet

# NetherNet Server
cargo run --example server -p nethernet

# NetherNet Client
cargo run --example client -p nethernet
```

## Requirements

- Rust 1.85 or higher

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
