//! Chain Module Benchmarks
//!
//! Run with: `cargo bench`

use beebotos_chain::cache::PersistentCache;
use beebotos_chain::compat::{Address, B256, U256};
use beebotos_chain::security::SecurityValidator;
use beebotos_chain::wallet::{HDWallet, Wallet};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Benchmark wallet operations
fn bench_wallet(c: &mut Criterion) {
    let mut group = c.benchmark_group("wallet");

    group.bench_function("wallet_random", |b| {
        b.iter(|| {
            let wallet = Wallet::random();
            black_box(wallet.address());
        });
    });

    group.bench_function("hd_wallet_from_mnemonic", |b| {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                        abandon abandon about";
        b.iter(|| {
            let wallet = HDWallet::from_mnemonic(mnemonic).unwrap();
            black_box(wallet.derive_account(0, None).unwrap());
        });
    });

    group.bench_function("hd_wallet_derive", |b| {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                        abandon abandon about";
        let wallet = HDWallet::from_mnemonic(mnemonic).unwrap();
        let mut index = 0u32;
        b.iter(|| {
            let account = wallet.derive_account(index, None).unwrap();
            black_box(account.address);
            index += 1;
        });
    });

    group.finish();
}

/// Benchmark cache operations
fn bench_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache");

    group.bench_function("cache_put", |b| {
        let cache = PersistentCache::<String, String>::new(1000);
        let mut index = 0u64;
        b.iter(|| {
            cache.put(
                format!("key{}", index),
                format!("value{}", index),
                Some(3600),
            );
            index += 1;
        });
    });

    group.bench_function("cache_get", |b| {
        let cache = PersistentCache::<String, String>::new(1000);
        for i in 0..100 {
            cache.put(format!("key{}", i), format!("value{}", i), None);
        }
        let mut index = 0u64;
        b.iter(|| {
            let key = format!("key{}", index % 100);
            black_box(cache.get(&key));
            index += 1;
        });
    });

    group.bench_function("cache_put_get", |b| {
        let cache = PersistentCache::<String, String>::new(1000);
        let mut index = 0u64;
        b.iter(|| {
            let key = format!("key{}", index);
            cache.put(key.clone(), format!("value{}", index), None);
            black_box(cache.get(&key));
            index += 1;
        });
    });

    group.finish();
}

/// Benchmark security validation
fn bench_security(c: &mut Criterion) {
    let mut group = c.benchmark_group("security");

    group.bench_function("security_validate", |b| {
        let validator = SecurityValidator::new().max_gas_price(100_000_000_000);
        let to = Address::from([1u8; 20]);
        b.iter(|| {
            black_box(validator.validate_transaction(to, 1000, 20_000_000_000, &[]));
        });
    });

    group.bench_function("input_sanitize_address", |b| {
        b.iter(|| {
            black_box(beebotos_chain::security::InputSanitizer::sanitize_address(
                "0x1234567890123456789012345678901234567890",
            ));
        });
    });

    group.finish();
}

/// Benchmark address operations
fn bench_address(c: &mut Criterion) {
    let mut group = c.benchmark_group("address");

    group.bench_function("address_parse", |b| {
        let addr_str = "0x1234567890123456789012345678901234567890";
        b.iter(|| {
            black_box(addr_str.parse::<Address>());
        });
    });

    group.bench_function("address_to_string", |b| {
        let addr = Address::from([1u8; 20]);
        b.iter(|| {
            black_box(addr.to_string());
        });
    });

    group.bench_function("b256_from_slice", |b| {
        let slice = [1u8; 32];
        b.iter(|| {
            black_box(B256::from(slice));
        });
    });

    group.finish();
}

/// Benchmark U256 operations
fn bench_u256(c: &mut Criterion) {
    let mut group = c.benchmark_group("u256");

    group.bench_function("u256_add", |b| {
        let a = U256::from(1000000);
        let c = U256::from(500000);
        b.iter(|| {
            black_box(a + c);
        });
    });

    group.bench_function("u256_mul", |b| {
        let a = U256::from(1000000);
        let c = U256::from(500000);
        b.iter(|| {
            black_box(a * c);
        });
    });

    group.bench_function("u256_div", |b| {
        let a = U256::from(1000000);
        let c = U256::from(500);
        b.iter(|| {
            black_box(a / c);
        });
    });

    group.bench_function("u256_pow", |b| {
        let a = U256::from(2);
        b.iter(|| {
            black_box(a.pow(U256::from(10)));
        });
    });

    group.finish();
}

/// Benchmark proposal building
fn bench_proposal(c: &mut Criterion) {
    let mut group = c.benchmark_group("proposal");

    group.bench_function("proposal_build", |b| {
        use beebotos_chain::dao::ProposalBuilder;

        let target = Address::from([1u8; 20]);
        b.iter(|| {
            let builder =
                ProposalBuilder::new("Test Proposal").add_transfer(target, U256::from(1000));
            black_box(builder.build());
        });
    });

    group.finish();
}

/// Throughput benchmarks
fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    // Cache throughput
    group.throughput(criterion::Throughput::Elements(1000));
    group.bench_function("cache_ops_per_sec", |b| {
        let cache = PersistentCache::<String, String>::new(10000);
        let mut index = 0u64;
        b.iter(|| {
            for _ in 0..1000 {
                let key = format!("key{}", index);
                cache.put(key.clone(), format!("value{}", index), None);
                cache.get(&key);
                index += 1;
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_wallet,
    bench_cache,
    bench_security,
    bench_address,
    bench_u256,
    bench_proposal,
    bench_throughput
);
criterion_main!(benches);
