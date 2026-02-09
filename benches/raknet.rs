use bytes::{Buf, BufMut, Bytes, BytesMut};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use raknet::protocol::{
    ack::{AckNackPayload, SequenceRange},
    constants::{DEFAULT_UNCONNECTED_MAGIC, DatagramFlags},
    datagram::{Datagram, DatagramPayload},
    encapsulated_packet::{EncapsulatedPacket, SplitInfo},
    packet::{Packet, RaknetEncodable, UnconnectedPing, UnconnectedPong},
    reliability::Reliability,
    types::{Advertisement, DatagramHeader, EncapsulatedPacketHeader, RaknetTime, Sequence24},
};
use std::hint::black_box;
use std::time::Duration;

fn create_test_datagram(packet_count: usize) -> Datagram {
    let packets = (0..packet_count)
        .map(|i| EncapsulatedPacket {
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
        })
        .collect();

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

fn create_test_split_packet() -> EncapsulatedPacket {
    EncapsulatedPacket {
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
    }
}

fn create_test_ack_payload(range_count: usize) -> AckNackPayload {
    let ranges = (0..range_count)
        .map(|i| SequenceRange {
            start: Sequence24::new(i as u32 * 10),
            end: Sequence24::new(i as u32 * 10 + 5),
        })
        .collect();

    AckNackPayload { ranges }
}

fn bench_datagram_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("datagram_encode");

    for count in [4, 8, 16] {
        let datagram = create_test_datagram(count);

        let mut tmp = BytesMut::with_capacity(1400);
        datagram.encode(&mut tmp).unwrap();

        group.throughput(Throughput::Bytes(tmp.len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &datagram,
            |b, datagram| {
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

    for count in [4, 8, 16] {
        let datagram = create_test_datagram(count);

        let mut buf = BytesMut::with_capacity(1400);
        datagram.encode(&mut buf).unwrap();

        let encoded = buf.freeze();

        group.throughput(Throughput::Bytes(encoded.len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &encoded,
            |b, encoded| {
                b.iter_batched(
                    || encoded.clone(),
                    |data| {
                        let mut d = data;
                        let pkt = Datagram::decode(black_box(&mut d)).unwrap();
                        black_box(pkt);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_datagram_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("datagram_roundtrip");

    for count in [4, 8, 16] {
        let datagram = create_test_datagram(count);

        let mut tmp = BytesMut::with_capacity(1400);
        datagram.encode(&mut tmp).unwrap();

        group.throughput(Throughput::Bytes(tmp.len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &datagram,
            |b, datagram| {
                b.iter(|| {
                    let mut buf = BytesMut::with_capacity(1400);
                    datagram.encode(black_box(&mut buf)).unwrap();

                    let mut data = buf.freeze();

                    let pkt = Datagram::decode(&mut data).unwrap();
                    black_box(pkt);
                });
            },
        );
    }

    group.finish();
}

fn bench_encapsulated_packet_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("encapsulated_packet_encode");

    for size in [64, 256, 512, 1024, 2048] {
        let packet = create_test_encapsulated_packet(size);

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &packet, |b, packet| {
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

    for size in [512, 1024, 2048] {
        let packet = create_test_encapsulated_packet(size);

        let mut buf = BytesMut::with_capacity(size + 64);
        packet.encode_raknet(&mut buf).unwrap();

        let encoded = buf.freeze();

        group.throughput(Throughput::Bytes(encoded.len() as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &encoded, |b, encoded| {
            b.iter_batched(
                || encoded.clone(),
                |data| {
                    let mut d = data;
                    let pkt = EncapsulatedPacket::decode_raknet(black_box(&mut d)).unwrap();

                    black_box(pkt);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_encapsulated_packet_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("encapsulated_packet_roundtrip");

    for size in [512, 1024, 2048] {
        let packet = create_test_encapsulated_packet(size);

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &packet, |b, packet| {
            b.iter(|| {
                let mut buf = BytesMut::with_capacity(size + 64);

                packet.encode_raknet(black_box(&mut buf)).unwrap();

                let mut data = buf.freeze();

                let pkt = EncapsulatedPacket::decode_raknet(&mut data).unwrap();

                black_box(pkt);
            });
        });
    }

    group.finish();
}

fn bench_split_packet(c: &mut Criterion) {
    let mut group = c.benchmark_group("split_packet");

    let packet = create_test_split_packet();

    let mut buf = BytesMut::with_capacity(128);
    packet.encode_raknet(&mut buf).unwrap();

    let encoded = buf.freeze();

    group.bench_function("encode", |b| {
        b.iter(|| {
            let mut b2 = BytesMut::with_capacity(128);
            packet.encode_raknet(black_box(&mut b2)).unwrap();
            black_box(b2);
        });
    });

    group.bench_function("decode", |b| {
        b.iter_batched(
            || encoded.clone(),
            |data| {
                let mut d = data;
                let pkt = EncapsulatedPacket::decode_raknet(black_box(&mut d)).unwrap();

                black_box(pkt);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("roundtrip", |b| {
        b.iter(|| {
            let mut b2 = BytesMut::with_capacity(128);

            packet.encode_raknet(black_box(&mut b2)).unwrap();

            let mut d = b2.freeze();

            let pkt = EncapsulatedPacket::decode_raknet(&mut d).unwrap();

            black_box(pkt);
        });
    });

    group.finish();
}

fn bench_unconnected_packets(c: &mut Criterion) {
    let mut group = c.benchmark_group("unconnected_packets");

    let ping = UnconnectedPing {
        ping_time: RaknetTime(1000),
        magic: DEFAULT_UNCONNECTED_MAGIC,
    };

    let mut ping_buf = BytesMut::with_capacity(32);
    ping_buf.put_u8(UnconnectedPing::ID);
    ping.encode_body(&mut ping_buf).unwrap();

    let encoded_ping = ping_buf.freeze();

    let ad = Bytes::from("MCPE;Test Server;390;1.14.60;0;20;12345;World;Survival;1;19132;19133;");

    let pong = UnconnectedPong {
        ping_time: RaknetTime(1000),
        server_guid: 0x1234567890abcdef,
        magic: DEFAULT_UNCONNECTED_MAGIC,
        advertisement: Advertisement(Some(ad)),
    };

    let mut pong_buf = BytesMut::with_capacity(256);
    pong_buf.put_u8(UnconnectedPong::ID);
    pong.encode_body(&mut pong_buf).unwrap();

    let encoded_pong = pong_buf.freeze();

    group.bench_function("ping_encode", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(32);

            buf.put_u8(UnconnectedPing::ID);

            ping.encode_body(black_box(&mut buf)).unwrap();

            black_box(buf);
        });
    });

    group.bench_function("ping_decode", |b| {
        b.iter_batched(
            || encoded_ping.clone(),
            |mut data| {
                data.get_u8();

                let pkt = UnconnectedPing::decode_body(black_box(&mut data)).unwrap();

                black_box(pkt);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("pong_encode", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(256);

            buf.put_u8(UnconnectedPong::ID);

            pong.encode_body(black_box(&mut buf)).unwrap();

            black_box(buf);
        });
    });

    group.bench_function("pong_decode", |b| {
        b.iter_batched(
            || encoded_pong.clone(),
            |mut data| {
                data.get_u8();

                let pkt = UnconnectedPong::decode_body(black_box(&mut data)).unwrap();

                black_box(pkt);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_ack_payload(c: &mut Criterion) {
    let mut group = c.benchmark_group("ack_payload");

    for count in [20, 50] {
        let payload = create_test_ack_payload(count);

        let mut buf = BytesMut::with_capacity(256);
        payload.encode_raknet(&mut buf).unwrap();

        let encoded = buf.freeze();

        group.bench_with_input(BenchmarkId::new("encode", count), &payload, |b, payload| {
            b.iter(|| {
                let mut b2 = BytesMut::with_capacity(256);

                payload.encode_raknet(black_box(&mut b2)).unwrap();

                black_box(b2);
            });
        });

        group.bench_with_input(BenchmarkId::new("decode", count), &encoded, |b, encoded| {
            b.iter_batched(
                || encoded.clone(),
                |data| {
                    let mut d = data;

                    let pkt = AckNackPayload::decode_raknet(black_box(&mut d)).unwrap();

                    black_box(pkt);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_sequence24(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequence24");

    let seq = Sequence24::new(0x123456);

    let mut buf = BytesMut::with_capacity(3);
    seq.encode_raknet(&mut buf).unwrap();

    let encoded = buf.freeze();

    group.bench_function("encode", |b| {
        b.iter(|| {
            let mut b2 = BytesMut::with_capacity(3);

            seq.encode_raknet(black_box(&mut b2)).unwrap();

            black_box(b2);
        });
    });

    group.bench_function("decode", |b| {
        b.iter_batched(
            || encoded.clone(),
            |data| {
                let mut d = data;

                let v = Sequence24::decode_raknet(black_box(&mut d)).unwrap();

                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_buffer_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_allocation");

    for size in [256, 512, 1024, 1400, 2048] {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter(|| {
                let buf = BytesMut::with_capacity(black_box(size));

                black_box(buf);
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(150)
        .warm_up_time(Duration::from_secs(3));
    targets =
        bench_datagram_encode,
        bench_datagram_decode,
        bench_datagram_roundtrip,
        bench_encapsulated_packet_encode,
        bench_encapsulated_packet_decode,
        bench_encapsulated_packet_roundtrip,
        bench_split_packet,
        bench_unconnected_packets,
        bench_ack_payload,
        bench_sequence24,
        bench_buffer_allocation
}

criterion_main!(benches);
