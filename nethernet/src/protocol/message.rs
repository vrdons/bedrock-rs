use crate::error::{NethernetError, Result};
use crate::protocol::constants::MAX_MESSAGE_SIZE;
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Message segment
/// First byte contains segment count, remainder contains data
#[derive(Debug, Clone)]
pub struct MessageSegment {
    /// Remaining segment count (0 = last segment)
    pub remaining_segments: u8,
    /// Segment data
    pub data: Bytes,
}

impl MessageSegment {
    pub fn new(remaining_segments: u8, data: Bytes) -> Self {
        Self {
            remaining_segments,
            data,
        }
    }

    /// Encodes the segment to bytes
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(1 + self.data.len());
        buf.put_u8(self.remaining_segments);
        buf.put(self.data.clone());
        buf.freeze()
    }

    /// Decodes a segment from bytes
    pub fn decode(mut data: Bytes) -> Result<Self> {
        if data.len() < 2 {
            return Err(NethernetError::MessageParse(
                "Message too short, expected at least 2 bytes".to_string(),
            ));
        }

        let remaining_segments = data.get_u8();
        Ok(Self {
            remaining_segments,
            data,
        })
    }
}

/// Complete message - data assembled from segments
#[derive(Debug, Clone)]
pub struct Message {
    /// Expected segment count
    expected_segments: u8,
    /// Assembled data
    data: BytesMut,
}

impl Message {
    pub fn new() -> Self {
        Self {
            expected_segments: 0,
            data: BytesMut::new(),
        }
    }

    /// Adds a segment and checks if the message is complete
    pub fn add_segment(&mut self, segment: MessageSegment) -> Result<Option<Bytes>> {
        // Set expected_segments if this is the first segment
        if self.expected_segments == 0 && segment.remaining_segments > 0 {
            self.expected_segments = segment.remaining_segments + 1;
        }

        // Check segment order
        if self.expected_segments > 0 {
            let expected_remaining = self.expected_segments - 1;
            if expected_remaining != segment.remaining_segments {
                // Reset state before returning error to keep Message instance safe for reuse
                self.data.clear();
                self.expected_segments = 0;
                return Err(NethernetError::MessageParse(format!(
                    "Invalid segment sequence: expected {}, got {}",
                    expected_remaining, segment.remaining_segments
                )));
            }
            self.expected_segments -= 1;
        }

        // Add data
        self.data.put(segment.data);

        // Return message if this is the last segment
        if segment.remaining_segments == 0 {
            let data = self.data.split().freeze();
            self.expected_segments = 0;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Splits the message into segments
    pub fn split_into_segments(data: Bytes) -> Result<Vec<MessageSegment>> {
        if data.len() <= MAX_MESSAGE_SIZE {
            return Ok(vec![MessageSegment::new(0, data)]);
        }

        let mut segments = Vec::new();
        let mut remaining = data;

        // Calculate segment count
        let segment_count = remaining.len().div_ceil(MAX_MESSAGE_SIZE);

        // Ensure segment count fits in u8
        if segment_count > 255 {
            return Err(NethernetError::MessageTooLarge(remaining.len()));
        }

        let mut segments_left = segment_count as u8;

        while !remaining.is_empty() {
            segments_left -= 1;
            let chunk_size = remaining.len().min(MAX_MESSAGE_SIZE);
            let chunk = remaining.split_to(chunk_size);
            segments.push(MessageSegment::new(segments_left, chunk));
        }

        Ok(segments)
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_segment() {
        let data = Bytes::from("Hello, World!");
        let segments = Message::split_into_segments(data.clone()).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].remaining_segments, 0);
        assert_eq!(segments[0].data, data);
    }

    #[test]
    fn test_multiple_segments() {
        let data = Bytes::from(vec![0u8; MAX_MESSAGE_SIZE * 2 + 100]);
        let segments = Message::split_into_segments(data.clone()).unwrap();

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].remaining_segments, 2);
        assert_eq!(segments[1].remaining_segments, 1);
        assert_eq!(segments[2].remaining_segments, 0);
    }

    #[test]
    fn test_reassembly() {
        let original_data = Bytes::from(vec![1u8; MAX_MESSAGE_SIZE * 2 + 100]);
        let segments = Message::split_into_segments(original_data.clone()).unwrap();

        let mut message = Message::new();
        for (i, segment) in segments.iter().enumerate() {
            let result = message.add_segment(segment.clone()).unwrap();
            if i < segments.len() - 1 {
                assert!(result.is_none());
            } else {
                assert!(result.is_some());
                assert_eq!(result.unwrap(), original_data);
            }
        }
    }

    #[test]
    fn test_out_of_order_segments_error() {
        // Create multiple segments from a large data
        let data = Bytes::from(vec![42u8; MAX_MESSAGE_SIZE * 2 + 100]);
        let segments = Message::split_into_segments(data.clone()).unwrap();

        // Should have at least 3 segments
        assert!(segments.len() >= 3);

        let mut message = Message::new();

        // Add segment 1 (middle segment) first - this should succeed
        // because it's the first segment being added and sets expected_segments
        let result = message.add_segment(segments[1].clone());
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // Not complete yet

        // Now try to add segment 0 (earlier segment with higher remaining_segments)
        // This should fail because we expect the next segment in sequence
        let result = message.add_segment(segments[0].clone());
        assert!(result.is_err());

        // Verify it's the right error type
        if let Err(NethernetError::MessageParse(msg)) = result {
            assert!(msg.contains("Invalid segment sequence"));
        } else {
            panic!("Expected MessageParse error for out-of-order segment");
        }
    }
}
