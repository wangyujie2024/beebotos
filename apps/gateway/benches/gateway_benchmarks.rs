//! Gateway Performance Benchmarks
//!
//! Run with: cargo bench -p beebotos-gateway

use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Benchmark API key hashing
fn bench_api_key_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("api_key_hashing");

    group.bench_function("hash_api_key", |b| {
        let key = "test-api-key-12345";
        b.iter(|| {
            // Simulate the hash computation
            let hash = format!("{:x}", md5::compute(black_box(key)));
            black_box(hash);
        });
    });

    group.finish();
}

/// Benchmark JWT token validation
fn bench_jwt_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("jwt_validation");

    group.bench_function("parse_token", |b| {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0";
        b.iter(|| {
            // Simulate token parsing
            let parts: Vec<&str> = black_box(token).split('.').collect();
            black_box(parts);
        });
    });

    group.finish();
}

/// Benchmark webhook signature verification
fn bench_webhook_signature(c: &mut Criterion) {
    let mut group = c.benchmark_group("webhook_signature");

    group.bench_function("verify_hmac", |b| {
        let secret = "test-secret";
        let payload = b"test payload data";

        b.iter(|| {
            // Simulate HMAC computation
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            black_box(secret).hash(&mut hasher);
            black_box(payload).hash(&mut hasher);
            let hash = hasher.finish();
            black_box(hash);
        });
    });

    group.finish();
}

/// Benchmark JSON serialization
fn bench_json_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_serialization");

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestMessage {
        id: String,
        content: String,
        timestamp: u64,
    }

    let message = TestMessage {
        id: "msg-123".to_string(),
        content: "Test message content".to_string(),
        timestamp: 1234567890,
    };

    group.bench_function("serialize", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&message)).unwrap();
            black_box(json);
        });
    });

    let json = serde_json::to_string(&message).unwrap();
    group.bench_function("deserialize", |b| {
        b.iter(|| {
            let msg: TestMessage = serde_json::from_str(black_box(&json)).unwrap();
            black_box(msg);
        });
    });

    group.finish();
}

/// Benchmark capability parsing
fn bench_capability_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("capability_parsing");

    group.bench_function("parse_capability_string", |b| {
        let cap = "llm:4000:openai,anthropic";
        b.iter(|| {
            let parts: Vec<&str> = black_box(cap).split(':').collect();
            black_box(parts);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_api_key_hashing,
    bench_jwt_validation,
    bench_webhook_signature,
    bench_json_serialization,
    bench_capability_parsing
);
criterion_main!(benches);
