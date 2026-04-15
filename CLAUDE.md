# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BeeBotOS is a Web4.0 autonomous agent operating system written in Rust. It uses a Cargo workspace with core crates (`crates/`), applications (`apps/`), Solidity smart contracts (`contracts/`), and Protocol Buffer definitions (`proto/`).

### Workspace Layout

- **`crates/`** — Core libraries (11 crates). Key ones:
  - `core` — Shared types, event bus, error types, and configuration primitives; lowest in the dependency graph.
  - `kernel` — OS kernel (preemptive scheduler, capability-based security with 11 levels, WASM runtime with WASI, 29 syscalls).
  - `brain` — NEAT evolution, PAD emotion dynamics, OCEAN personality, and memory systems.
  - `agents` — Agent runtime, A2A protocol, MCP integration, service mesh with DID resolver, and channel communication abstractions.
  - `chain` — Blockchain integrations (Ethereum, BSC, Polygon, Solana) via `alloy`; includes HD wallet support.
  - `gateway-lib` — Shared HTTP/Web infrastructure (Axum-based) consumed by `apps/gateway`. Provides `AgentRuntime` trait, `StateStore` (CQRS), auth middleware, rate limiting, WebSocket management, and service discovery.
  - `message-bus` — Internal message bus with persistence and transport layers used across crates.
  - `crypto`, `p2p`, `sdk`, `telemetry` — Cryptographic primitives, peer-to-peer networking, public SDK, and observability/metrics.
- **`apps/`** — Binaries and services:
  - `gateway` — Main API gateway service (binds port `8000`). Business logic handlers live here; HTTP infrastructure comes from `gateway-lib`.
  - `web` — Web management frontend using **Leptos** (CSR/WASM). Includes a `web-server` binary for serving the static site (binds port `8090`).
  - `cli` — Command-line tool (`beebot`).
  - `beehub` — Hub service.
- **`contracts/`** — Solidity smart contracts managed with Foundry.
- **`tests/`** — Integration and end-to-end tests, plus loose test files (`test_*.rs`) at the top level.
- **`migrations_sqlite/`** — SQLite migrations used by `apps/gateway` (`sqlx::migrate!("../../migrations_sqlite")`).
- **`proto/`** — Protocol Buffer definitions (A2A, kernel, brain, DAO, payment, skills). Not actively code-generated at this time.

### Architecture Constraint

`crates/agents` **must not** depend directly on web frameworks (e.g., `axum`, `actix-web`, `rocket`, `warp`, `tide`, `salvo`). All HTTP-related infrastructure must go through `crates/gateway-lib`. This is enforced as a project convention; `crates/agents/Cargo.toml` correctly depends on `beebotos-gateway-lib` rather than raw web framework crates.

Dependency direction:
```
                    apps/gateway
                         |
                         v
              crates/gateway-lib -> crates/core
                    |     |
                    v     v
        crates/agents  crates/kernel
              |              |
              +------+-------+
                     |
              crates/chain, etc.
```

`apps/gateway` wires everything together: it uses `gateway-lib` for HTTP infrastructure, `beebotos-agents` for agent business logic (via `AgentService`), and `beebotos-kernel` for sandboxed task execution.

## Key Architectural Patterns

### Gateway-Lib as the HTTP Abstraction Layer
`crates/gateway-lib` is the single source of truth for HTTP concerns across the workspace. It exports:
- `AgentRuntime` trait — how the gateway interacts with agents without coupling to `agents` internals.
- `StateStore` — CQRS-style state management for agent lifecycles.
- Middleware (`auth_middleware`, `rate_limit_middleware`, `trace_layer`, `cors_layer`).
- Rate limiting (token bucket, sliding window, fixed window).
- WebSocket management (`WebSocketManager`).
- Service discovery and load balancing.

When adding new HTTP handlers in `apps/gateway`, reuse these abstractions rather than introducing new raw Axum middleware or state management.

### Agent Runtime and Kernel Integration
The `agents` crate integrates with the kernel through `kernel_integration.rs`. Agents can be executed inside the kernel's WASM sandbox. The kernel exposes:
- `KernelBuilder` — fluent builder for kernel configuration.
- `CapabilitySet` / `CapabilityLevel` — 11-level capability-based security.
- `Scheduler` — preemptive task scheduler with priorities.
- `wasm::WasmEngine` — WASM compilation and instantiation.
- 29 syscalls under `kernel::syscalls`.

### Service Mesh and DID
`crates/agents/src/service_mesh/` provides a unified service registry with chain-based DID resolution. The `AgentServiceMesh` is constructed via `ServiceMeshBuilder` and can resolve agent identities on-chain.

### Message Bus
`beebotos-message-bus` is used by `kernel`, `agents`, `gateway`, and `chain`. It supports both in-memory and persistent transports. Common usage pattern:
```rust
use beebotos_message_bus::{MessageBus, MessageRouter};
```

## Toolchain

- **Rust**: nightly channel (see `rust-toolchain.toml`).
- **Targets**: `wasm32-unknown-unknown`, Windows cross-compile targets.
- **Contracts**: Foundry (`forge`).
- **Task runner**: `just` (preferred) or `make`.

## Common Commands

### Build

```bash
# Debug build entire workspace
cargo build --workspace

# Release build
cargo build --workspace --release

# Build a specific crate or app
cargo build -p beebotos-kernel
cargo build -p beebotos-gateway

# Build web frontend (WASM target)
cargo build -p beebotos-web --target wasm32-unknown-unknown
```

### Run Services

```bash
# Run the API gateway
cargo run -p beebotos-gateway

# Run the web frontend server
cargo run -p beebotos-web --bin web-server

# Install the CLI locally
cargo install --path apps/cli --force
```

### Test

```bash
# Run all workspace tests with all features
cargo test --workspace --all-features

# Run tests for a single crate
cargo test -p beebotos-agents

# Run a specific test by name (workspace-wide)
cargo test --workspace test_name -- --nocapture

# Run a specific test in a specific crate
cargo test -p beebotos-kernel test_name -- --nocapture

# Run unit tests only (libs)
cargo test --workspace --lib

# Run integration tests
cargo test --workspace --test '*'
```

### Code Quality

```bash
# Format (uses rustfmt.toml)
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Clippy (strict; uses clippy.toml)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Dependency audit
cargo deny check
cargo audit
```

### Smart Contracts (Foundry)

```bash
cd contracts

# Build
forge build

# Test
forge test

# Format
forge fmt
```

### Just / Make Shortcuts

```bash
just build          # Release build
just test           # All tests
just test-filter FILTER  # Run tests matching FILTER workspace-wide
just check          # fmt + clippy + test
just dev            # cargo watch -x build -x test
just contract-test  # forge test in contracts/
just install        # Install CLI locally
just fmt            # Format code
just lint           # Run clippy only
just doc            # Generate workspace docs (no deps)
just clean          # cargo clean + remove target/ and dist/
just coverage       # Generate HTML coverage report with tarpaulin
just release        # Production release build (--locked)
```

Make equivalents exist in `Makefile` for the same targets.

### Debug / Profile Builds

The workspace defines a custom profile useful for profiling release-like code with debug symbols:

```bash
cargo build --workspace --profile release-with-debug
```

## Database

`apps/gateway` uses **SQLite** by default. Migrations are loaded from `migrations_sqlite/` at runtime via `sqlx::migrate!("../../migrations_sqlite")`. The `migrations/` directory at the repo root also contains schema files (some with `_sqlite` suffix), but the runtime source of truth for the gateway is `migrations_sqlite/`.

## Configuration & Environment

- Gateway reads its configuration via `apps/gateway/src/config.rs` and supports an interactive config wizard (`config_wizard.rs`).
- Common env vars referenced across the codebase: `DATABASE_URL`, `RUST_LOG`, `JWT_SECRET`, `KIMI_API_KEY`, `LARK_APP_ID`, `LARK_APP_SECRET`.
- Gateway fixed port: `8000`. Web frontend fixed port: `8090`.

## File & Directory Conventions

### Data files (`data/`)
All module data files must be centralized under the `data/` directory (relative to the runtime working directory). Do not scatter data files in home directories (`~/.beebotos`) or next to source files.

Expected paths:
- `data/beebotos.db` — Gateway SQLite database
- `data/memory_search.db` — Agent memory search database
- `data/search_index.db` — Markdown memory search index
- `data/workspace/` — Markdown memory workspace
- `data/media/` — Downloaded media files
- `data/logs/` — Application logs
- `data/skills/` — Installed skill packages
- `data/personal_wechat_session.json` — Personal WeChat iLink session

When adding new persistence or caching, default to a path under `data/`.

### Configuration files (`config/`)
All runtime configuration files must be placed under the `config/` directory. Do not place config files at the repo root or inside individual crate source trees.

### Sensitive information (`.env`)
All secrets, API keys, tokens, passwords, and private keys must be stored in a `.env` file (loaded via `dotenvy` or similar) and must never be hard-coded in source files or committed to version control. Ensure `.env` is listed in `.gitignore`.

## Notes for Agents

- When working on `crates/agents`, do not introduce direct dependencies on `axum` or other web frameworks. Use `beebotos-gateway-lib` for HTTP concerns.
- The agents crate integrates with `beebotos-kernel` (features: `wasm`) for sandboxed execution and `beebotos-message-bus` for internal messaging.
- A2A protocol modules live under `crates/agents/src/a2a/`.
- The kernel exposes a builder pattern (`KernelBuilder`) and capability-based security with 11 levels.
