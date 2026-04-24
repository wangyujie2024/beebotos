//! Testing utilities for BeeBotOS Agents
//!
//! 🆕 OPTIMIZATION: Provides mocks and test helpers for planning integration
//! tests

pub mod mocks;

// Re-export commonly used mocks
/// Re-export test helpers for convenience
pub use helpers::{
    create_complex_task, create_simple_task, create_task_with_planning, create_test_plan,
    create_test_plan_with_deps,
};
pub use mocks::{helpers, MockPlanExecutor, MockPlanningEngine, MockRePlanner};
