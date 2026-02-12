use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use tracing::warn;

use crate::protocol::{
    datagram::Datagram,
    packet::{DecodeError, EncodeError},
};

/// A codec for decoding and encoding RakNet datagrams over UDP.
///
/// This codec is designed to be used with [`tokio_util::udp::UdpFramed`].
#[allow(dead_code)]
pub struct RaknetCodec;

impl Decoder for RaknetCodec {
    type Item = Datagram;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        match Datagram::decode(src) {
            Ok(datagram) => {
                // UdpFramed delivers exactly one UDP payload per frame, Datagram::decode consumes
                // the packet contents, and therefore we clear the src buffer after successful
                // decode to mark the frame consumed.
                src.clear();
                Ok(Some(datagram))
            }
            Err(e) => {
                // Log the error and swallow it to prevent terminating the stream
                // unless it's a fatal error (currently all decode errors are treated as non-fatal
                // for the stream itself, just dropping the malformed packet).
                // UdpFramed consumers will only see Err for fatal failures.
                warn!("Failed to decode datagram: {:?}. Dropping packet.", e);
                src.clear();
                Ok(None)
            }
        }
    }
}

impl Encoder<Datagram> for RaknetCodec {
    type Error = EncodeError;

    fn encode(&mut self, item: Datagram, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode(dst)
    }
}
