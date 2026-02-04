//! Packet handling and segmentation.

mod segmentation;

pub use segmentation::{
    SegmentedMessage,
    SegmentAssembler,
    segment_packet,
};
