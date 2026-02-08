use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nethernet::protocol::packet::discovery::{
    MessagePacket, RequestPacket, ResponsePacket, marshal, unmarshal,
};
use nethernet::protocol::{Message, MessageSegment};
use std::hint::black_box;

const SENDER_ID: u64 = 0x1234567890abcdef;

// Message encoding/decoding benchmarks
fn bench_message_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_encode");

    for size in [64, 256, 512, 1024, 4096].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let payload = vec![0u8; size];
            let segment = MessageSegment {
                remaining_segments: 0,
                data: Bytes::from(payload),
            };

            b.iter(|| {
                let encoded = segment.encode();
                black_box(encoded);
            });
        });
    }
    group.finish();
}

fn bench_message_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_decode");

    for size in [64, 256, 512, 1024, 4096].iter() {
        let payload = vec![0u8; *size];
        let segment = MessageSegment {
            remaining_segments: 0,
            data: Bytes::from(payload),
        };
        let encoded = segment.encode();

        group.throughput(Throughput::Bytes(encoded.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &encoded, |b, encoded| {
            b.iter(|| {
                let decoded = MessageSegment::decode(black_box(encoded)).unwrap();
                black_box(decoded);
            });
        });
    }
    group.finish();
}

// Message segmentation benchmarks
fn bench_message_segmentation(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_segmentation");

    for size in [1024, 4096, 8192, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let payload = Bytes::from(vec![0u8; size]);

            b.iter(|| {
                let segments = Message::split_into_segments(black_box(payload.clone())).unwrap();
                black_box(segments);
            });
        });
    }
    group.finish();
}

// Discovery packet marshaling benchmarks (includes encryption + checksum)
fn bench_discovery_marshal(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_marshal");

    // Request packet (minimal size)
    group.bench_function("request_packet", |b| {
        let packet = RequestPacket;
        let sender_id = SENDER_ID;

        b.iter(|| {
            let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
            black_box(marshaled);
        });
    });

    // Response packet with small application data
    for size in [128, 512, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("response_packet", size),
            size,
            |b, &size| {
                let packet = ResponsePacket::new(vec![0u8; size]);
                let sender_id = SENDER_ID;

                b.iter(|| {
                    let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
                    black_box(marshaled);
                });
            },
        );
    }

    // Message packet with varying data sizes
    for size in [64, 256, 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("message_packet", size),
            size,
            |b, &size| {
                let data_str = "x".repeat(size);
                let packet = MessagePacket::new(0x9876543210fedcba, data_str);
                let sender_id = SENDER_ID;

                b.iter(|| {
                    let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
                    black_box(marshaled);
                });
            },
        );
    }

    group.finish();
}

// Discovery packet unmarshaling benchmarks (includes decryption + checksum verification)
fn bench_discovery_unmarshal(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_unmarshal");

    // Request packet
    group.bench_function("request_packet", |b| {
        let packet = RequestPacket;
        let sender_id = SENDER_ID;
        let marshaled = marshal(&packet, sender_id).unwrap();

        b.iter(|| {
            let (unmarshaled, _sender) = unmarshal(black_box(&marshaled)).unwrap();
            black_box(unmarshaled);
        });
    });

    // Response packet with varying application data sizes
    for size in [128, 512, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("response_packet", size),
            size,
            |b, &size| {
                let packet = ResponsePacket::new(vec![0u8; size]);
                let sender_id = SENDER_ID;
                let marshaled = marshal(&packet, sender_id).unwrap();

                b.iter(|| {
                    let (unmarshaled, _sender) = unmarshal(black_box(&marshaled)).unwrap();
                    black_box(unmarshaled);
                });
            },
        );
    }

    // Message packet with varying data sizes
    for size in [64, 256, 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("message_packet", size),
            size,
            |b, &size| {
                let data_str = "x".repeat(size);
                let packet = MessagePacket::new(0x9876543210fedcba, data_str);
                let sender_id = SENDER_ID;
                let marshaled = marshal(&packet, sender_id).unwrap();

                b.iter(|| {
                    let (unmarshaled, _sender) = unmarshal(black_box(&marshaled)).unwrap();
                    black_box(unmarshaled);
                });
            },
        );
    }

    group.finish();
}

// Full discovery packet round-trip (marshal + unmarshal)
// This tests the complete encryption/decryption + checksum cycle
fn bench_discovery_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("discovery_roundtrip");

    // Request packet
    group.bench_function("request_packet", |b| {
        let packet = RequestPacket;
        let sender_id = SENDER_ID;

        b.iter(|| {
            let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
            let (unmarshaled, _sender) = unmarshal(&marshaled).unwrap();
            black_box(unmarshaled);
        });
    });

    // Response packet with varying application data sizes
    for size in [128, 512, 2048].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("response_packet", size),
            size,
            |b, &size| {
                let packet = ResponsePacket::new(vec![0u8; size]);
                let sender_id = SENDER_ID;

                b.iter(|| {
                    let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
                    let (unmarshaled, _sender) = unmarshal(&marshaled).unwrap();
                    black_box(unmarshaled);
                });
            },
        );
    }

    // Message packet with varying data sizes
    for size in [64, 256, 1024].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(
            BenchmarkId::new("message_packet", size),
            size,
            |b, &size| {
                let data_str = "x".repeat(size);
                let packet = MessagePacket::new(0x9876543210fedcba, data_str);
                let sender_id = SENDER_ID;

                b.iter(|| {
                    let marshaled = marshal(black_box(&packet), black_box(sender_id)).unwrap();
                    let (unmarshaled, _sender) = unmarshal(&marshaled).unwrap();
                    black_box(unmarshaled);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_message_encode,
    bench_message_decode,
    bench_message_segmentation,
    bench_discovery_marshal,
    bench_discovery_unmarshal,
    bench_discovery_roundtrip
);

criterion_main!(benches);
