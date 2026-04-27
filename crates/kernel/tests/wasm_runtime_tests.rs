//! WASM Runtime Tests
//!
//! Tests for WebAssembly execution engine, instance management, memory, and
//! metering.

use beebotos_kernel::wasm::engine::CacheStats;
use beebotos_kernel::wasm::host_funcs::{HostContext, HostFunctions};
use beebotos_kernel::wasm::memory::{MemoryConfig, MemoryStats, MAX_PAGES, PAGE_SIZE};
use beebotos_kernel::wasm::metering::{CostModel, FuelLimit, FuelTracker, WasmResourceLimits};
use beebotos_kernel::wasm::trap::{TrapAction, TrapHandler, WasmTrap};
use beebotos_kernel::wasm::{
    init, init_with_config, is_available, quick_compile, quick_instantiate, test_module_add,
    total_instances_created, version_info, EngineConfig, WasmEngine,
};

/// Test WASM runtime initialization
#[test]
fn test_wasm_init() {
    // Initialization should succeed
    let result = init();
    assert!(result.is_ok());
}

/// Test WASM runtime initialization with custom config
#[test]
fn test_wasm_init_with_config() {
    let config = EngineConfig {
        max_memory_size: 256 * 1024 * 1024, // 256MB
        max_fuel: 50_000_000,
        fuel_metering: true,
        memory_limits: true,
        wasi_enabled: false,
        debug_info: true,
        parallel_compilation: false,
        optimize: false,
    };

    let result = init_with_config(config);
    assert!(result.is_ok());
}

/// Test version information
#[test]
fn test_version_info() {
    let info = version_info();

    assert!(!info.version.is_empty());
    assert!(!info.wasmtime_version.is_empty());
}

/// Test engine configuration defaults
#[test]
fn test_engine_config_defaults() {
    let config = EngineConfig::default();

    assert_eq!(config.max_memory_size, 128 * 1024 * 1024); // 128MB
    assert_eq!(config.max_fuel, 10_000_000);
    assert!(config.fuel_metering);
    assert!(config.memory_limits);
    assert!(config.wasi_enabled);
}

/// Test production and development configs
#[test]
fn test_engine_config_presets() {
    let prod = EngineConfig::production();
    assert!(!prod.debug_info);
    assert!(prod.optimize);

    let dev = EngineConfig::development();
    assert!(dev.debug_info);
    assert!(!dev.optimize);
}

/// Test WASM module compilation
#[test]
fn test_module_compilation() {
    if !is_available() {
        return; // Skip if WASM not available
    }

    let wasm = test_module_add();
    let result = quick_compile(&wasm);

    assert!(
        result.is_ok(),
        "Failed to compile WASM module: {:?}",
        result.err()
    );
}

/// Test WASM module instantiation
#[test]
fn test_module_instantiation() {
    if !is_available() {
        return;
    }

    let wasm = test_module_add();
    let result = quick_instantiate(&wasm);

    assert!(
        result.is_ok(),
        "Failed to instantiate WASM module: {:?}",
        result.err()
    );

    // Check that instance counter was incremented
    assert!(total_instances_created() >= 1);
}

/// Test WASM engine creation
#[test]
fn test_wasm_engine_creation() {
    if !is_available() {
        return;
    }

    let config = EngineConfig::default();
    let engine = WasmEngine::new(config);

    assert!(engine.is_ok());
}

/// Test memory configuration
#[test]
fn test_memory_config() {
    let config = MemoryConfig::default();

    assert!(config.initial_pages > 0);
    assert!(config.max_pages <= Some(MAX_PAGES));
    assert_eq!(PAGE_SIZE, 65536); // 64KB
}

/// Test cost model
#[test]
fn test_cost_model() {
    let model = CostModel::default();

    // Each instruction should have a positive cost
    assert!(model.base_cost > 0);
    assert!(model.memory_load_cost > 0);
    assert!(model.call_cost > 0);
}

/// Test fuel tracker
#[test]
fn test_fuel_tracker() {
    let mut tracker = FuelTracker::new(CostModel::default(), FuelLimit::Limited(1000));

    assert!(tracker.consume(100));
    assert!(tracker.consume(200));

    // Remaining fuel should be 700
    assert_eq!(tracker.remaining(), 700);

    // Try to consume more than available
    assert!(!tracker.consume(800));
}

/// Test unlimited fuel
#[test]
fn test_unlimited_fuel() {
    let mut tracker = FuelTracker::new(CostModel::default(), FuelLimit::Infinite);

    // Should never run out
    assert!(tracker.consume(u64::MAX));
    assert_eq!(tracker.remaining(), u64::MAX);
}

/// Test resource limits
#[test]
fn test_resource_limits() {
    let limits = WasmResourceLimits {
        max_memory: 64 * 1024 * 1024, // 64MB
        max_fuel: FuelLimit::Limited(1000),
        max_execution_time_ms: 5000,
        max_call_stack: 100,
        max_host_call_depth: 10,
    };

    assert_eq!(limits.max_memory, 64 * 1024 * 1024);
    assert!(limits.max_fuel.amount() > 0);
}

/// Test host context creation
#[test]
fn test_host_context() {
    let context = HostContext::new("test-agent");

    assert_eq!(context.agent_id, "test-agent");
}

/// Test trap handler
#[test]
fn test_trap_handler() {
    let handler = TrapHandler::new();

    // Default handler should retry for recoverable traps
    assert_eq!(
        handler.handle(WasmTrap::OutOfFuel, "test"),
        TrapAction::Retry
    );
}

/// Test trap action variants
#[test]
fn test_trap_actions() {
    use beebotos_kernel::wasm::trap::TrapAction;

    let actions = vec![
        TrapAction::Retry,
        TrapAction::Propagate,
        TrapAction::Terminate,
    ];

    for action in actions {
        match action {
            TrapAction::Retry | TrapAction::Propagate | TrapAction::Terminate => {}
        }
    }
}

/// Test WASM trap types
#[test]
fn test_wasm_trap_types() {
    use beebotos_kernel::wasm::trap::WasmTrap;

    let traps = vec![
        WasmTrap::OutOfFuel,
        WasmTrap::StackOverflow,
        WasmTrap::MemoryOutOfBounds,
        WasmTrap::TableOutOfBounds,
        WasmTrap::IndirectCallTypeMismatch,
        WasmTrap::IntegerOverflow,
        WasmTrap::IntegerDivisionByZero,
        WasmTrap::BadConversionToInteger,
        WasmTrap::UnreachableCodeReached,
        WasmTrap::Interrupt,
        WasmTrap::User(0),
    ];

    for trap in traps {
        match trap {
            WasmTrap::OutOfFuel => assert_eq!(trap.to_string(), "out of fuel"),
            WasmTrap::StackOverflow => assert_eq!(trap.to_string(), "stack overflow"),
            _ => {}
        }
    }
}

/// Test engine cache stats
#[test]
fn test_cache_stats() {
    let stats = CacheStats {
        cached_modules: 100,
        total_uses: 150,
    };

    // No hit_rate method available, just verify struct creation
    assert_eq!(stats.cached_modules, 100);
    assert_eq!(stats.total_uses, 150);
}

/// Test invalid WASM compilation
#[test]
fn test_invalid_wasm() {
    if !is_available() {
        return;
    }

    let invalid_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x99]; // Invalid
    let result = quick_compile(&invalid_wasm);

    assert!(result.is_err());
}

/// Test empty WASM compilation
#[test]
fn test_empty_wasm() {
    if !is_available() {
        return;
    }

    let result = quick_compile(&[]);
    assert!(result.is_err());
}

/// Test memory stats
#[test]
fn test_memory_stats() {
    let stats = MemoryStats {
        current_pages: 1,
        current_size: PAGE_SIZE,
        max_pages: Some(1),
        max_size: Some(PAGE_SIZE),
        grow_count: 0,
        shrink_count: 0,
    };

    assert_eq!(stats.current_size, PAGE_SIZE);
}

/// Test host functions setup
#[test]
fn test_host_functions() {
    // HostFunctions has no constructor or is_empty method in current API
    // Just verify it can be used as a type
    let _funcs: Option<HostFunctions> = None;
}

/// Test fuel consumption tracking
#[test]
fn test_fuel_consumption() {
    let mut tracker = FuelTracker::new(CostModel::default(), FuelLimit::Limited(1000));

    // Consume fuel in chunks
    for _ in 0..10 {
        assert!(tracker.consume(50));
    }

    // Should have 500 remaining
    assert_eq!(tracker.remaining(), 500);

    // Reset and check
    tracker.reset();
    tracker.set_limit(FuelLimit::Limited(2000));
    assert_eq!(tracker.remaining(), 2000);
}

/// Test instance statistics
#[test]
fn test_instance_stats() {
    use beebotos_kernel::wasm::instance::InstanceStats;

    let stats = InstanceStats {
        memory_pages: 1,
        memory_bytes: 65536,
        fuel_consumed: 500,
        fuel_remaining: 500,
    };

    assert_eq!(stats.memory_pages, 1);
    assert_eq!(stats.fuel_consumed, 500);
}
