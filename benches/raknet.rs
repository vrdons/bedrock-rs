use bytes::{Buf, BufMut, Bytes, BytesMut};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use raknet::protocol::{
    ack::{AckNackPayload, SequenceRange},
    constants::{DatagramFlags, DEFAULT_UNCONNECTED_MAGIC},
    datagram::{Datagram, DatagramPayload},
    encapsulated_packet::{EncapsulatedPacket, SplitInfo},
    packet::{RaknetEncodable, UnconnectedPing, UnconnectedPong, Packet},
    reliability::Reliability,
    types::{Advertisement, DatagramHeader, EncapsulatedPacketHeader, RaknetTime, Sequence24},
};
use std::hint::black_box;

// Helper builders used by multiple benchmarks
fn create_test_datagram(packet_count: usize) -> Datagram {
    let mut packets = Vec::new();
    for i in 0..packet_count {
        packets.push(EncapsulatedPacket {
            header: EncapsulatedPacketHeader {
                reliability: Reliability::ReliableOrdered,
                is_split: false,
                needs_bas: false,
            },
            bit_length: 512,
            reliable_index: Some(Sequence24::new(i as u32)),
            sequence_index: None,
            ordering_index: Some(Sequence24::new(i as u32)),
            ordering_channel: Some(0),
            split: None,
            payload: Bytes::from(vec![0u8; 64]),
        });
    }

    Datagram {
        header: DatagramHeader {
            flags: DatagramFlags::VALID,
            sequence: Sequence24::new(0),
        },
        payload: DatagramPayload::EncapsulatedPackets(packets),
    }
}

fn create_test_encapsulated_packet(size: usize) -> EncapsulatedPacket {
    EncapsulatedPacket {
        header: EncapsulatedPacketHeader {
            reliability: Reliability::ReliableOrdered,
            is_split: false,
            needs_bas: false,
        },
        bit_length: (size * 8) as u16,
        reliable_index: Some(Sequence24::new(0)),
        sequence_index: None,
        ordering_index: Some(Sequence24::new(0)),
        ordering_channel: Some(0),
        split: None,
        payload: Bytes::from(vec![0u8; size]),
    }
}

fn create_test_ack_payload(range_count: usize) -> AckNackPayload {
    let mut ranges = Vec::new();
    for i in 0..range_count {
        ranges.push(SequenceRange {
            start: Sequence24::new(i as u32 * 10),
            end: Sequence24::new(i as u32 * 10 + 5),
        });
    }
    AckNackPayload { ranges }
}

// Datagram encoding/decoding benchmarks
fn bench_datagram_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("datagram_encode");

    for packet_count in [1, 4, 8, 16].iter() {
        group.throughput(Throughput::Bytes((*packet_count as u64) * 64));
        group.bench_with_input(
            BenchmarkId::from_parameter(packet_count),
            packet_count,
            |b, &count| {
                let datagram = create_test_datagram(count);

                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(1400);
                    datagram.encode(black_box(&mut buf)).unwrap();
                    black_box(buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_datagram_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("datagram_decode");

    for packet_count in [1, 4, 8, 16].iter() {
        group.throughput(Throughput::Bytes((*packet_count as u64) * 64));
        group.bench_with_input(
            BenchmarkId::from_parameter(packet_count),
            packet_count,
            |b, &count| {
                let datagram = create_test_datagram(count);

                let mut buf = BytesMut::with_capacity(1400);
                datagram.encode(&mut buf).unwrap();
                let encoded = buf.freeze();

                b.iter(|| {
                    let mut data = black_box(encoded.clone());
                    let decoded = Datagram::decode(black_box(&mut data)).unwrap();
                    black_box(decoded);
                });
            },
        );
    }
    group.finish();
}

// Datagram round-trip benchmarks (encode + decode)
fn bench_datagram_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("datagram_roundtrip");

    for packet_count in [1, 4, 8, 16].iter() {
        group.throughput(Throughput::Bytes((*packet_count as u64) * 64));
        group.bench_with_input(
            BenchmarkId::from_parameter(packet_count),
            packet_count,
            |b, &count| {
                let datagram = create_test_datagram(count);

                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(1400);
                    datagram.encode(black_box(&mut buf)).unwrap();
                    let encoded = buf.freeze();
                    let mut data = encoded;
                    let decoded = Datagram::decode(&mut data).unwrap();
                    black_box(decoded);
                });
            },
        );
    }
    group.finish();
}

// EncapsulatedPacket encoding/decoding benchmarks
fn bench_encapsulated_packet_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encapsulated_packet_encode");

    for size in [64, 256, 512, 1024, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let packet = create_test_encapsulated_packet(size);

            b.iter(|| {
                let mut buf = BytesMut::with_capacity(size + 64);
                packet.encode_raknet(black_box(&mut buf)).unwrap();
                black_box(buf);
            });
        });
    }
    group.finish();
}

fn bench_encapsulated_packet_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encapsulated_packet_decode");

    for size in [64, 256, 512, 1024, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let packet = create_test_encapsulated_packet(size);

            let mut buf = BytesMut::with_capacity(size + 64);
            packet.encode_raknet(&mut buf).unwrap();
            let encoded = buf.freeze();

            b.iter(|| {
                let mut data = black_box(encoded.clone());
                let decoded = EncapsulatedPacket::decode_raknet(black_box(&mut data)).unwrap();
                black_box(decoded);
            });
        });
    }
    group.finish();
}

// EncapsulatedPacket round-trip benchmarks
fn bench_encapsulated_packet_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("encapsulated_packet_roundtrip");

    for size in [64, 256, 512, 1024, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let packet = create_test_encapsulated_packet(size);

            b.iter(|| {
                let mut buf = BytesMut::with_capacity(size + 64);
                packet.encode_raknet(black_box(&mut buf)).unwrap();
                let encoded = buf.freeze();
                let mut data = encoded;
                let decoded = EncapsulatedPacket::decode_raknet(&mut data).unwrap();
                black_box(decoded);
            });
        });
    }
    group.finish();
}

// Split packet benchmarks
fn bench_split_packet_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("split_packet_encode");

    group.bench_function("with_split_info", |b| {
        let packet = EncapsulatedPacket {
            header: EncapsulatedPacketHeader {
                reliability: Reliability::ReliableOrdered,
                is_split: true,
                needs_bas: false,
            },
            bit_length: 512,
            reliable_index: Some(Sequence24::new(0)),
            sequence_index: None,
            ordering_index: Some(Sequence24::new(0)),
            ordering_channel: Some(0),
            split: Some(SplitInfo {
                count: 10,
                id: 1,
                index: 5,
            }),
            payload: Bytes::from(vec![0u8; 64]),
        };

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(128);
            packet.encode_raknet(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    group.finish();
}

fn bench_split_packet_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("split_packet_decode");

    group.bench_function("with_split_info", |b| {
        let packet = EncapsulatedPacket {
            header: EncapsulatedPacketHeader {
                reliability: Reliability::ReliableOrdered,
                is_split: true,
                needs_bas: false,
            },
            bit_length: 512,
            reliable_index: Some(Sequence24::new(0)),
            sequence_index: None,
            ordering_index: Some(Sequence24::new(0)),
            ordering_channel: Some(0),
            split: Some(SplitInfo {
                count: 10,
                id: 1,
                index: 5,
            }),
            payload: Bytes::from(vec![0u8; 64]),
        };

        let mut buf = BytesMut::with_capacity(128);
        packet.encode_raknet(&mut buf).unwrap();
        let encoded = buf.freeze();

        b.iter(|| {
            let mut data = black_box(encoded.clone());
            let decoded = EncapsulatedPacket::decode_raknet(black_box(&mut data)).unwrap();
            black_box(decoded);
        });
    });

    group.finish();
}

fn bench_split_packet_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("split_packet_roundtrip");

    group.bench_function("with_split_info", |b| {
        let packet = EncapsulatedPacket {
            header: EncapsulatedPacketHeader {
                reliability: Reliability::ReliableOrdered,
                is_split: true,
                needs_bas: false,
            },
            bit_length: 512,
            reliable_index: Some(Sequence24::new(0)),
            sequence_index: None,
            ordering_index: Some(Sequence24::new(0)),
            ordering_channel: Some(0),
            split: Some(SplitInfo {
                count: 10,
                id: 1,
                index: 5,
            }),
            payload: Bytes::from(vec![0u8; 64]),
        };

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(128);
            packet.encode_raknet(black_box(&mut buf)).unwrap();
            let encoded = buf.freeze();
            let mut data = encoded;
            let decoded = EncapsulatedPacket::decode_raknet(&mut data).unwrap();
            black_box(decoded);
        });
    });

    group.finish();
}

// Unconnected packet benchmarks (ping/pong discovery)
fn bench_unconnected_packets(c: &mut Criterion) {
    let mut group = c.benchmark_group("unconnected_packets");
    // Shared ping/pong values and pre-encoded buffers for decode benches
    let ping = UnconnectedPing {
        ping_time: RaknetTime(1000),
        magic: DEFAULT_UNCONNECTED_MAGIC,
    };

    let mut ping_buf = BytesMut::with_capacity(32);
    ping_buf.put_u8(UnconnectedPing::ID);
    ping.encode_body(&mut ping_buf).unwrap();
    let encoded_ping = ping_buf.freeze();

    let ad_data = Bytes::from("MCPE;Test Server;390;1.14.60;0;20;12345;World;Survival;1;19132;19133;");
    let pong = UnconnectedPong {
        ping_time: RaknetTime(1000),
        server_guid: 0x1234567890abcdef,
        magic: DEFAULT_UNCONNECTED_MAGIC,
        advertisement: Advertisement(Some(ad_data.clone())),
    };

    let mut pong_buf = BytesMut::with_capacity(256);
    pong_buf.put_u8(UnconnectedPong::ID);
    pong.encode_body(&mut pong_buf).unwrap();
    let encoded_pong = pong_buf.freeze();

    // UnconnectedPing encode
    group.bench_function("ping_encode", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(32);
            buf.put_u8(UnconnectedPing::ID);
            ping.encode_body(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    // UnconnectedPing decode
    group.bench_function("ping_decode", |b| {
        b.iter(|| {
            let mut data = black_box(encoded_ping.clone());
            data.get_u8(); // Skip ID
            let decoded = UnconnectedPing::decode_body(black_box(&mut data)).unwrap();
            black_box(decoded);
        });
    });

    // UnconnectedPong encode
    group.bench_function("pong_encode", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(256);
            buf.put_u8(UnconnectedPong::ID);
            pong.encode_body(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    // UnconnectedPong decode
    group.bench_function("pong_decode", |b| {
        b.iter(|| {
            let mut data = black_box(encoded_pong.clone());
            data.get_u8(); // Skip ID
            let decoded = UnconnectedPong::decode_body(black_box(&mut data)).unwrap();
            black_box(decoded);
        });
    });

    // Ping/Pong round-trip
    group.bench_function("ping_pong_roundtrip", |b| {
        b.iter(|| {
            // Encode ping
            let mut buf = BytesMut::with_capacity(32);
            buf.put_u8(UnconnectedPing::ID);
            ping.encode_body(black_box(&mut buf)).unwrap();
            let encoded = buf.freeze();

            // Decode ping
            let mut data = encoded;
            data.get_u8();
            let decoded = UnconnectedPing::decode_body(&mut data).unwrap();
            black_box(decoded);
        });
    });

    group.finish();
}

// ACK/NACK payload benchmarks
fn bench_ack_payload_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("ack_payload_encode");

    for range_count in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(range_count),
            range_count,
            |b, &count| {
                let payload = create_test_ack_payload(count);

                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(256);
                    payload.encode_raknet(black_box(&mut buf)).unwrap();
                    black_box(buf);
                });
            },
        );
    }
    group.finish();
}

fn bench_ack_payload_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("ack_payload_decode");

    for range_count in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(range_count),
            range_count,
            |b, &count| {
                let payload = create_test_ack_payload(count);
                let mut buf = BytesMut::with_capacity(256);
                payload.encode_raknet(&mut buf).unwrap();
                let encoded = buf.freeze();

                b.iter(|| {
                    let mut data = black_box(encoded.clone());
                    let decoded = AckNackPayload::decode_raknet(black_box(&mut data)).unwrap();
                    black_box(decoded);
                });
            },
        );
    }
    group.finish();
}

// ACK/NACK round-trip benchmarks
fn bench_ack_payload_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("ack_payload_roundtrip");

    for range_count in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(range_count),
            range_count,
            |b, &count| {
                let payload = create_test_ack_payload(count);

                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(256);
                    payload.encode_raknet(black_box(&mut buf)).unwrap();
                    let encoded = buf.freeze();
                    let mut data = encoded;
                    let decoded = AckNackPayload::decode_raknet(&mut data).unwrap();
                    black_box(decoded);
                });
            },
        );
    }
    group.finish();
}

// Sequence24 operations benchmarks
fn bench_sequence24_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence24_operations");

    group.bench_function("encode", |b| {
        let seq = Sequence24::new(0x123456);

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(3);
            seq.encode_raknet(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    group.bench_function("decode", |b| {
        let seq = Sequence24::new(0x123456);
        let mut buf = BytesMut::with_capacity(3);
        seq.encode_raknet(&mut buf).unwrap();
        let encoded = buf.freeze();

        b.iter(|| {
            let mut data = black_box(encoded.clone());
            let decoded = Sequence24::decode_raknet(black_box(&mut data)).unwrap();
            black_box(decoded);
        });
    });

    group.bench_function("roundtrip", |b| {
        let seq = Sequence24::new(0x123456);

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(3);
            seq.encode_raknet(black_box(&mut buf)).unwrap();
            let encoded = buf.freeze();
            let mut data = encoded;
            let decoded = Sequence24::decode_raknet(&mut data).unwrap();
            black_box(decoded);
        });
    });

    group.finish();
}

// Reliability type benchmarks
fn bench_reliability_checks(c: &mut Criterion) {
    let mut group = c.benchmark_group("reliability_checks");

    let reliabilities = [
        Reliability::Unreliable,
        Reliability::UnreliableSequenced,
        Reliability::Reliable,
        Reliability::ReliableOrdered,
        Reliability::ReliableSequenced,
    ];

    for reliability in reliabilities.iter() {
        group.bench_with_input(
            BenchmarkId::new("is_reliable", format!("{:?}", reliability)),
            reliability,
            |b, &rel| {
                b.iter(|| {
                    let result = black_box(rel).is_reliable();
                    black_box(result);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_ordered", format!("{:?}", reliability)),
            reliability,
            |b, &rel| {
                b.iter(|| {
                    let result = black_box(rel).is_ordered();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

// SequenceRange operations benchmarks
fn bench_sequence_range_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence_range_operations");

    group.bench_function("non_wrapping_encode", |b| {
        let range = SequenceRange {
            start: Sequence24::new(100),
            end: Sequence24::new(200),
        };

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(16);
            range.encode_raknet(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    group.bench_function("wrapping_encode", |b| {
        let range = SequenceRange {
            start: Sequence24::new(0x00FF_FFFE),
            end: Sequence24::new(2),
        };

        b.iter(|| {
            let mut buf = BytesMut::with_capacity(32);
            range.encode_raknet(black_box(&mut buf)).unwrap();
            black_box(buf);
        });
    });

    group.bench_function("wrapping_check", |b| {
        let range = SequenceRange {
            start: Sequence24::new(0x00FF_FFFE),
            end: Sequence24::new(2),
        };

        b.iter(|| {
            let result = black_box(range).wraps();
            black_box(result);
        });
    });

    group.bench_function("split_wrapping", |b| {
        let range = SequenceRange {
            start: Sequence24::new(0x00FF_FFFE),
            end: Sequence24::new(2),
        };

        b.iter(|| {
            let result = black_box(range).split_wrapping();
            black_box(result);
        });
    });

    group.finish();
}

// Buffer allocation benchmarks
fn bench_buffer_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_allocation");

    for size in [256, 512, 1024, 1400, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let buf = BytesMut::with_capacity(black_box(size));
                black_box(buf);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_datagram_encode,
    bench_datagram_decode,
    bench_datagram_roundtrip,
    bench_encapsulated_packet_encode,
    bench_encapsulated_packet_decode,
    bench_encapsulated_packet_roundtrip,
    bench_split_packet_encode,
    bench_split_packet_decode,
    bench_split_packet_roundtrip,
    bench_unconnected_packets,
    bench_ack_payload_encode,
    bench_ack_payload_decode,
    bench_ack_payload_roundtrip,
    bench_sequence24_operations,
    bench_reliability_checks,
    bench_sequence_range_operations,
    bench_buffer_allocation
);

criterion_main!(benches);
