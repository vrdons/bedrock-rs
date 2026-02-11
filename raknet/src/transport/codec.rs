use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use crate::protocol::{
    datagram::Datagram,
    packet::{DecodeError, EncodeError},
};

/// A codec for decoding and encoding RakNet datagrams over UDP.
///
/// This codec is designed to be used with [`tokio_util::udp::UdpFramed`].
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
                // Datagram::decode might leave trailing bytes if the packet has padding we didn't consume properly,
                // or if it was malformed but partially valid.
                // However, `UdpFramed` works frame-by-frame. The `src` buffer contains exactly one UDP packet payload.
                // We should consume the entire buffer or consider it an error if there's significant data left?
                // RakNet datagrams can have padding at the end? `Datagram::decode` handles packets until EOF.
                // So if we return Ok, we assume we consumed what we needed.
                // We should clear the buffer to signal we're done with this frame, just in case.
                src.clear();
                Ok(Some(datagram))
            }
            Err(e) => {
                // If decoding fails, we discard the buffer.
                src.clear();
                // We might want to return the error or swallow it.
                // Returning the error might terminate the stream depending on how it's handled.
                // For a server, we probably don't want to crash on a bad packet.
                // But let's return it and handle it in the stream consumer.
                Err(e)
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
