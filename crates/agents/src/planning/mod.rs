//! Agent Planning Module
//!
//! Provides comprehensive planning capabilities for autonomous agents:
//! - Hierarchical task decomposition
//! - Multi-strategy planning (ReAct, Chain-of-Thought, Goal-based)
//! - Dynamic plan adaptation and replanning
//! - Plan execution with dependency management
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Planning Module                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │   Planner    │  │   Decomposer │  │   RePlanner  │          │
//! │  │   Engine     │──│              │──│              │          │
//! │  └──────┬───────┘  └──────────────┘  └──────────────┘          │
//! │         │                                                       │
//! │         ▼                                                       │
//! │  ┌─────────────────────────────────────────────┐               │
//! │  │              Plan Executor                   │               │
//! │  ├──────────────┬──────────────┬───────────────┤               │
//! │  │   Sequential │   Parallel   │   Adaptive    │               │
//! │  │   Execution  │  Execution   │  Execution    │               │
//! │  └──────────────┴──────────────┴───────────────┘               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! ```ignore
//! use beebotos_agents::planning::{PlanningEngine, PlanStrategy, PlanContext};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create planning engine
//!     let engine = PlanningEngine::new();
//!     
//!     // Create a plan
//!     let plan = engine.quick_plan("Analyze system performance").await?;
//!     
//!     println!("Created plan with {} steps", plan.steps.len());
//!     
//!     Ok(())
//! }
//! ```

// Sub-modules
pub mod decomposer;
pub mod engine;
pub mod executor;
pub mod plan;
pub mod replanner;

// ARCHITECTURE FIX: Plan storage module for persistence
pub mod storage;

// Re-export core plan types
// Re-export decomposer types
pub use decomposer::{
    CompositeDecomposer, Decomposer, DecompositionContext, DecompositionStrategy, DomainDecomposer,
    HierarchicalDecomposer, ParallelDecomposer, TaskDecomposer,
};
// Re-export engine types (additional)
pub use engine::PlannerToolRegistry;
// Re-export engine types
pub use engine::{
    ChainOfThoughtPlanner, GoalBasedPlanner, HybridPlanner, PlanContext, PlanStrategy, Planner,
    PlanningConfig, PlanningEngine, ReActPlanner,
};
// Re-export executor types
pub use executor::{
    ActionHandler, DefaultActionHandler, ExecutionConfig, ExecutionContext, ExecutionEvent,
    ExecutionResult, ExecutionStrategy, ParallelExecutor, PlanExecutor, SequentialExecutor,
    ToolExecutor,
};
pub use plan::{
    Action, Plan, PlanId, PlanStatus, PlanStep, PlanningError, PlanningResult, Priority,
    StepStatus, StepType,
};
// Re-export replanner types
pub use replanner::{
    AdaptationResult, AdaptationStrategy, CompositeRePlanner, ConditionRePlanner,
    FeedbackRePlanner, RePlanTrigger, RePlanner, ResourceRePlanner,
};
// ARCHITECTURE FIX: Re-export storage types
pub use storage::{
    FilePlanStorage, InMemoryPlanStorage, PlanFilter, PlanStorage, PlanStorageError,
    PlanStorageResult, StorageStats,
};
