//! Message segmentation for packets exceeding 10KB.
//!
//! Packets exceeding the MAX_MESSAGE_SIZE are split into multiple segments
//! that are transmitted across multiple SCTP messages.

use bytes::{Bytes, BytesMut, Buf, BufMut};
use crate::error::{NethernetError, Result};
use crate::protocol::constants::MAX_MESSAGE_SIZE;

/// Represents a segmented message.
#[derive(Debug, Clone)]
pub struct SegmentedMessage {
    /// Number of segments remaining (including this one)
    pub segments_remaining: u8,
    /// The packet data for this segment
    pub data: Bytes,
}

impl SegmentedMessage {
    /// Creates a new segmented message.
    pub fn new(segments_remaining: u8, data: Bytes) -> Self {
        Self {
            segments_remaining,
            data,
        }
    }

    /// Encodes the segmented message to bytes.
    ///
    /// Format:
    /// - Segment count (u8)
    /// - Packet length (varuint32)
    /// - Packet data (bytes)
    pub fn encode(&self) -> Result<Bytes> {
        let mut buf = BytesMut::new();
        
        // Write segment count
        buf.put_u8(self.segments_remaining);
        
        // Write packet length as varuint32
        write_varuint32(&mut buf, self.data.len() as u32);
        
        // Write packet data
        buf.put_slice(&self.data);
        
        Ok(buf.freeze())
    }

    /// Decodes a segmented message from bytes.
    pub fn decode(mut data: Bytes) -> Result<Self> {
        if data.is_empty() {
            return Err(NethernetError::Other("empty segment data".to_string()));
        }
        
        // Read segment count
        let segments_remaining = data.get_u8();
        
        // Read packet length as varuint32
        let packet_len = read_varuint32(&mut data)?;
        
        if data.len() < packet_len as usize {
            return Err(NethernetError::Other(
                format!("insufficient data: expected {}, got {}", packet_len, data.len())
            ));
        }
        
        // Extract packet data
        let packet_data = data.split_to(packet_len as usize);
        
        Ok(Self {
            segments_remaining,
            data: packet_data,
        })
    }
}

/// Simple message buffer for handling segmented messages (Go-style).
/// 
/// This is a simplified version matching the Go implementation's promise-based approach.
#[derive(Debug, Default)]
pub struct SegmentAssembler {
    /// Number of segments expected (promise)
    pub segments: u8,
    /// Accumulated data buffer
    pub data: Vec<u8>,
}

impl SegmentAssembler {
    /// Creates a new segment assembler.
    pub fn new() -> Self {
        Self {
            segments: 0,
            data: Vec::new(),
        }
    }

    /// Handles a message segment. Returns `Some(data)` when all segments have been received.
    ///
    /// This follows the Go implementation's simple promise-based approach:
    /// - Validates segment count matches promise (if segments > 0)
    /// - Updates promise and appends data
    /// - Returns complete message when segments == 0
    pub fn add_segment(&mut self, segment: SegmentedMessage) -> Result<Option<Bytes>> {
        // Validate promised segments
        if self.segments > 0 && self.segments - 1 != segment.segments_remaining {
            return Err(NethernetError::Other(
                format!("invalid promised segments: expected {}, got {}", self.segments - 1, segment.segments_remaining)
            ));
        }
        
        // Update promise
        self.segments = segment.segments_remaining;
        
        // Append data to buffer
        self.data.extend_from_slice(&segment.data);
        
        // If more segments expected, return None
        if self.segments > 0 {
            return Ok(None);
        }
        
        // All segments received - return complete message and clear buffer
        let complete_data = Bytes::from(std::mem::take(&mut self.data));
        Ok(Some(complete_data))
    }

    /// Resets the assembler state.
    pub fn reset(&mut self) {
        self.data.clear();
        self.segments = 0;
    }
}

/// Splits a packet into segments if it exceeds MAX_MESSAGE_SIZE.
pub fn segment_packet(data: Bytes) -> Vec<SegmentedMessage> {
    if data.len() <= MAX_MESSAGE_SIZE {
        // No segmentation needed
        return vec![SegmentedMessage::new(0, data)];
    }

    let mut segments = Vec::new();
    let mut remaining = data;
    let total_segments = (remaining.len() + MAX_MESSAGE_SIZE - 1) / MAX_MESSAGE_SIZE;

    for i in 0..total_segments {
        let segment_size = std::cmp::min(MAX_MESSAGE_SIZE, remaining.len());
        let segment_data = remaining.split_to(segment_size);
        let segments_remaining = (total_segments - i - 1) as u8;
        
        segments.push(SegmentedMessage::new(segments_remaining, segment_data));
    }

    segments.reverse(); // Send in reverse order so first segment has highest count
    segments
}

/// Writes a varuint32 value.
fn write_varuint32(buf: &mut BytesMut, mut value: u32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.put_u8(byte);
        if value == 0 {
            break;
        }
    }
}

/// Reads a varuint32 value.
fn read_varuint32(buf: &mut Bytes) -> Result<u32> {
    let mut value: u32 = 0;
    let mut shift = 0;

    loop {
        if buf.is_empty() {
            return Err(NethernetError::Other("unexpected end of varuint32".to_string()));
        }

        let byte = buf.get_u8();
        value |= ((byte & 0x7F) as u32) << shift;

        if (byte & 0x80) == 0 {
            break;
        }

        shift += 7;
        if shift >= 35 {
            return Err(NethernetError::Other("varuint32 too large".to_string()));
        }
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_segmentation() {
        let data = Bytes::from(vec![1, 2, 3, 4, 5]);
        let segments = segment_packet(data.clone());
        
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segments_remaining, 0);
        assert_eq!(segments[0].data, data);
    }

    #[test]
    fn test_segmentation() {
        let data = Bytes::from(vec![0u8; MAX_MESSAGE_SIZE * 2 + 100]);
        let segments = segment_packet(data.clone());
        
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].segments_remaining, 2);
        assert_eq!(segments[1].segments_remaining, 1);
        assert_eq!(segments[2].segments_remaining, 0);
    }

    #[test]
    fn test_segment_assembler() {
        let data = Bytes::from(vec![0u8; MAX_MESSAGE_SIZE * 2 + 100]);
        let segments = segment_packet(data.clone());
        
        let mut assembler = SegmentAssembler::new();
        
        for (i, segment) in segments.iter().enumerate() {
            let result = assembler.add_segment(segment.clone()).unwrap();
            if i < segments.len() - 1 {
                assert!(result.is_none());
            } else {
                assert_eq!(result.unwrap(), data);
            }
        }
    }

    #[test]
    fn test_varuint32_roundtrip() {
        let test_values = vec![0, 127, 128, 255, 256, 16383, 16384, 2097151, 2097152];
        
        for value in test_values {
            let mut buf = BytesMut::new();
            write_varuint32(&mut buf, value);
            let mut bytes = buf.freeze();
            let decoded = read_varuint32(&mut bytes).unwrap();
            assert_eq!(value, decoded);
        }
    }

    #[test]
    fn test_encode_decode_segment() {
        let segment = SegmentedMessage::new(2, Bytes::from(vec![1, 2, 3, 4, 5]));
        let encoded = segment.encode().unwrap();
        let decoded = SegmentedMessage::decode(encoded).unwrap();
        
        assert_eq!(segment.segments_remaining, decoded.segments_remaining);
        assert_eq!(segment.data, decoded.data);
    }
}
