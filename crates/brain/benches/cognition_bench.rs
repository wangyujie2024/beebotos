//! Cognitive System Benchmarks

use beebotos_brain::cognition::decision::{
    DecisionContext, DecisionOption, RiskLevel, TimeHorizon,
};
use beebotos_brain::cognition::{CognitiveState, Goal, MemoryItem, WorkingMemory};
use beebotos_brain::SocialBrainApi;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::json;

/// Benchmark working memory operations
fn bench_working_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("working_memory");

    // Benchmark add operation
    group.bench_function("add", |b| {
        let mut wm = WorkingMemory::new(100);
        let mut counter = 0;
        b.iter(|| {
            wm.add(MemoryItem {
                key: format!("key{}", counter),
                value: json!(counter),
                activation: 0.8,
                timestamp: counter as u64,
            });
            counter += 1;
        });
    });

    // Benchmark get operation
    group.bench_function("get", |b| {
        let mut wm = WorkingMemory::new(100);
        for i in 0..100 {
            wm.add(MemoryItem {
                key: format!("key{}", i),
                value: json!(i),
                activation: 0.8,
                timestamp: i as u64,
            });
        }

        let mut counter = 0;
        b.iter(|| {
            wm.get(&format!("key{}", black_box(counter % 100)));
            counter += 1;
        });
    });

    // Benchmark decay operation
    group.bench_function("decay", |b| {
        let mut wm = WorkingMemory::new(100);
        for i in 0..100 {
            wm.add(MemoryItem {
                key: format!("key{}", i),
                value: json!(i),
                activation: 0.5,
                timestamp: i as u64,
            });
        }

        b.iter(|| {
            wm.decay();
        });
    });

    group.finish();
}

/// Benchmark cognitive state operations
fn bench_cognitive_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("cognitive_state");

    // Benchmark set_goal
    group.bench_function("set_goal", |b| {
        let mut state = CognitiveState::new();
        let mut counter = 0;
        b.iter(|| {
            state.set_goal(Goal::new(&format!("Goal {}", black_box(counter)), 0.8));
            counter += 1;
        });
    });

    // Benchmark form_intention
    group.bench_function("form_intention", |b| {
        let mut state = CognitiveState::new();
        // Pre-populate with goals
        for i in 0..10 {
            state.set_goal(Goal::new(&format!("Goal {}", i), 0.8 - (i as f32 * 0.05)));
        }

        b.iter(|| {
            state.form_intention();
        });
    });

    group.finish();
}

/// Benchmark decision making
fn bench_decision_making(c: &mut Criterion) {
    let mut group = c.benchmark_group("decision_making");

    // Benchmark decide with varying numbers of options
    for num_options in [2, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("decide", num_options),
            num_options,
            |b, &num_options| {
                let api = SocialBrainApi::new();

                let options: Vec<DecisionOption> = (0..num_options)
                    .map(|i| DecisionOption {
                        id: format!("option_{}", i),
                        description: format!("Option {}", i),
                        expected_outcomes: vec![],
                        risk_level: RiskLevel::Medium,
                        time_horizon: TimeHorizon::ShortTerm,
                        resource_requirements: Default::default(),
                    })
                    .collect();

                let context = DecisionContext {
                    available_options: options,
                    constraints: vec![],
                    objectives: vec![],
                };

                b.iter(|| {
                    api.decide(black_box(&context));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark API cognitive operations
fn bench_api_cognition(c: &mut Criterion) {
    let mut group = c.benchmark_group("api_cognition");

    // Benchmark set_goal
    group.bench_function("set_goal", |b| {
        let mut api = SocialBrainApi::new();
        let mut counter = 0;
        b.iter(|| {
            api.set_goal(&format!("Goal {}", black_box(counter)), 0.8)
                .unwrap();
            counter += 1;
        });
    });

    // Benchmark form_intention
    group.bench_function("form_intention", |b| {
        let mut api = SocialBrainApi::new();
        for i in 0..10 {
            api.set_goal(&format!("Goal {}", i), 0.8).unwrap();
        }

        b.iter(|| {
            api.form_intention();
        });
    });

    // Benchmark add_to_working_memory
    group.bench_function("add_to_working_memory", |b| {
        let mut api = SocialBrainApi::new();
        let mut counter = 0;
        b.iter(|| {
            api.add_to_working_memory(
                &format!("key{}", black_box(counter)),
                json!({"value": counter}),
                0.8,
            );
            counter += 1;
        });
    });

    group.finish();
}

criterion_group!(
    cognition_benches,
    bench_working_memory,
    bench_cognitive_state,
    bench_decision_making,
    bench_api_cognition
);
criterion_main!(cognition_benches);
