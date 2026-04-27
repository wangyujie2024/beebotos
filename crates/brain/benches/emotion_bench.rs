//! Emotion System Benchmarks

use beebotos_brain::emotion::state::EmotionState;
use beebotos_brain::pad::{EmotionalEvent, EmotionalIntelligence, EmotionalTrait, Pad};
use beebotos_brain::personality::OceanProfile;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Benchmark PAD operations
fn bench_pad_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("pad");

    // Benchmark PAD creation
    group.bench_function("new", |b| {
        b.iter(|| {
            Pad::new(black_box(0.5), black_box(0.3), black_box(0.2));
        });
    });

    // Benchmark PAD distance calculation
    group.bench_function("distance", |b| {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);
        b.iter(|| {
            pad1.distance(black_box(&pad2));
        });
    });

    // Benchmark PAD interpolation (lerp)
    group.bench_function("lerp", |b| {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);
        let mut t = 0.0;
        b.iter(|| {
            pad1.lerp(black_box(&pad2), black_box(t));
            t = (t + 0.1) % 1.0;
        });
    });

    // Benchmark PAD clamping
    group.bench_function("clamp", |b| {
        let pad = Pad::new(2.0, -0.5, 1.5);
        b.iter(|| {
            black_box(pad.clone()).clamp();
        });
    });

    group.finish();
}

/// Benchmark emotional intelligence operations
fn bench_emotional_intelligence(c: &mut Criterion) {
    let mut group = c.benchmark_group("emotional_intelligence");

    // Benchmark creation
    group.bench_function("new", |b| {
        b.iter(|| {
            EmotionalIntelligence::new();
        });
    });

    // Benchmark event processing
    group.bench_function("update", |b| {
        let mut ei = EmotionalIntelligence::new();
        let event = EmotionalEvent {
            description: "Test event".to_string(),
            pleasure_impact: 0.5,
            arousal_impact: 0.3,
            dominance_impact: 0.2,
        };

        b.iter(|| {
            ei.update(black_box(&event));
        });
    });

    // Benchmark multiple sequential updates
    group.bench_function("update_10_events", |b| {
        let mut ei = EmotionalIntelligence::new();
        let events: Vec<EmotionalEvent> = (0..10)
            .map(|i| EmotionalEvent {
                description: format!("Event {}", i),
                pleasure_impact: rand::random::<f32>() * 2.0 - 1.0,
                arousal_impact: rand::random::<f32>(),
                dominance_impact: rand::random::<f32>(),
            })
            .collect();

        b.iter(|| {
            for event in &events {
                ei.update(black_box(event));
            }
        });
    });

    // Benchmark current state retrieval
    group.bench_function("current", |b| {
        let ei = EmotionalIntelligence::new();
        b.iter(|| {
            black_box(ei.current());
        });
    });

    group.finish();
}

/// Benchmark EmotionState operations
fn bench_emotion_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("emotion_state");

    // Benchmark creation
    group.bench_function("new", |b| {
        b.iter(|| {
            EmotionState::new(black_box(0.5), black_box(0.3), black_box(0.2));
        });
    });

    // Benchmark neutral creation
    group.bench_function("neutral", |b| {
        b.iter(|| {
            EmotionState::neutral();
        });
    });

    // Benchmark interpolation
    group.bench_function("lerp", |b| {
        let state1 = EmotionState::new(0.0, 0.0, 0.0);
        let state2 = EmotionState::new(1.0, 1.0, 1.0);
        let mut t = 0.0;

        b.iter(|| {
            state1.lerp(black_box(&state2), black_box(t));
            t = (t + 0.1) % 1.0;
        });
    });

    // Benchmark distance calculation
    group.bench_function("distance", |b| {
        let state1 = EmotionState::new(0.0, 0.0, 0.0);
        let state2 = EmotionState::new(1.0, 1.0, 1.0);

        b.iter(|| {
            state1.distance(black_box(&state2));
        });
    });

    // Benchmark blend (multiple emotions)
    for count in [2, 5, 10].iter() {
        group.bench_with_input(BenchmarkId::new("blend", count), count, |b, &count| {
            let emotions: Vec<(EmotionState, f64)> = (0..count)
                .map(|i| {
                    let t = i as f64 / count as f64;
                    (EmotionState::new(t, t, t), 1.0 / count as f64)
                })
                .collect();

            b.iter(|| {
                EmotionState::blend(black_box(&emotions));
            });
        });
    }

    group.finish();
}

/// Benchmark emotional trait operations
fn bench_emotional_traits(c: &mut Criterion) {
    let mut group = c.benchmark_group("emotional_traits");

    // Benchmark baseline offset calculation
    group.bench_function("baseline_offset", |b| {
        let traits = [
            EmotionalTrait::Optimistic,
            EmotionalTrait::Pessimistic,
            EmotionalTrait::HighEnergy,
            EmotionalTrait::LowEnergy,
            EmotionalTrait::Assertive,
            EmotionalTrait::Passive,
        ];
        let mut i = 0;

        b.iter(|| {
            traits[i % traits.len()].baseline_offset();
            i += 1;
        });
    });

    group.finish();
}

/// Benchmark personality-emotion interactions
fn bench_personality_emotion_interaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("personality_emotion");

    // Benchmark applying personality filter to emotion
    group.bench_function("personality_filter", |b| {
        let personality = OceanProfile::balanced();
        let mut emotion = EmotionState::neutral();

        b.iter(|| {
            // Simulate personality influence on emotion
            emotion.pleasure += (personality.neuroticism as f64 - 0.5) * 0.1;
            emotion.arousal += (personality.openness as f64 - 0.5) * 0.1;
        });
    });

    // Benchmark complete personality-emotion-stimulus cycle
    group.bench_function("full_cycle", |b| {
        let personality = OceanProfile::balanced();
        let mut emotion = EmotionState::neutral();
        let stimulus = EmotionalEvent {
            description: "Stimulus".to_string(),
            pleasure_impact: 0.5,
            arousal_impact: 0.3,
            dominance_impact: 0.2,
        };

        b.iter(|| {
            // Apply stimulus
            emotion.pleasure += stimulus.pleasure_impact;
            emotion.arousal += stimulus.arousal_impact;
            emotion.dominance += stimulus.dominance_impact;

            // Apply personality filter
            emotion.pleasure *= (1.0 - personality.neuroticism as f64 * 0.5);
            emotion.arousal *= (1.0 + personality.openness as f64 * 0.3);

            // Clamp to valid range
            emotion.pleasure = emotion.pleasure.clamp(-1.0, 1.0);
            emotion.arousal = emotion.arousal.clamp(0.0, 1.0);
            emotion.dominance = emotion.dominance.clamp(0.0, 1.0);

            black_box(&emotion);
        });
    });

    group.finish();
}

/// Benchmark emotion decay simulation
fn bench_emotion_decay(c: &mut Criterion) {
    let mut group = c.benchmark_group("emotion_decay");

    // Benchmark decay toward baseline
    group.bench_function("decay", |b| {
        let baseline = EmotionState::neutral();
        let mut emotion = EmotionState::new(0.8, 0.8, 0.8);
        let decay_rate = 0.1;

        b.iter(|| {
            emotion = emotion.lerp(black_box(&baseline), black_box(decay_rate));
        });
    });

    // Benchmark long-term decay simulation
    group.bench_function("decay_100_steps", |b| {
        let baseline = EmotionState::neutral();
        let decay_rate = 0.05;

        b.iter(|| {
            let mut emotion = EmotionState::new(0.8, 0.8, 0.8);
            for _ in 0..100 {
                emotion = emotion.lerp(&baseline, decay_rate);
            }
            black_box(emotion);
        });
    });

    group.finish();
}

criterion_group!(
    emotion_benches,
    bench_pad_operations,
    bench_emotional_intelligence,
    bench_emotion_state,
    bench_emotional_traits,
    bench_personality_emotion_interaction,
    bench_emotion_decay
);
criterion_main!(emotion_benches);
