# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Building

```bash
# Build core library (no_std by default)
cargo build

# Build with std feature
cargo build --features std

# Build with extensions (includes alloy for RPC interactions)
cargo build --features extensions

# Build with all features
cargo build --all-features
```

### Testing

```bash
# Run core tests
cargo test

# Run tests with std feature
cargo test --features std

# Run extension tests (must be single-threaded due to RPC state)
cargo test --features extensions --lib extensions -- --test-threads=1

# Run doc tests
cargo test --doc --all-features

# Run a specific test
cargo test test_name

# Run tests in a specific module
cargo test module_name::
```

### Linting and Formatting

```bash
# Run clippy (must pass with no warnings)
cargo clippy --all-targets --all-features -- -D warnings

# Check formatting
cargo fmt --all -- --check

# Apply formatting
cargo fmt --all
```

## High-Level Architecture

This is a Rust implementation of the Uniswap V4 SDK, maintaining API compatibility with the TypeScript SDK while
leveraging Rust's performance and safety features.

### Core Design Principles

- **no_std by default**: Library works without the standard library for embedded/WASM environments
- **Feature-gated extensions**: Optional functionality behind the `extensions` feature flag
- **Type safety**: Strong typing using Rust's type system and alloy-rs types
- **Integration with V3 SDK**: Builds upon and re-exports uniswap-v3-sdk functionality

### Module Structure

- **`abi`**: Solidity type definitions using alloy-sol-types for V4 contracts
- **`entities`**: Core domain models (Pool, Position, Route, Trade) that represent V4 concepts
- **`position_manager`**: NFT position management with V4-specific actions (mint, increase/decrease liquidity)
- **`utils`**: V4-specific utilities including:
    - `v4_planner`: Transaction planning for complex V4 operations
    - `v4_position_planner`: Position-specific transaction planning
    - `v4_base_actions_parser`: Parse and encode V4 actions
    - Price/tick conversions, currency handling, hook utilities
- **`extensions`** (feature-gated): Optional functionality requiring RPC access:
    - `pool_manager_lens`: Query pool state directly from contracts (like StateView)
    - `simple_tick_data_provider`: Fetch tick data via RPC calls

### Key Dependencies

- `alloy`: Ethereum types and contract interactions (optional, for extensions)
- `uniswap-sdk-core`: Core SDK functionality (currencies, tokens, etc.)
- `uniswap-v3-sdk`: V3 SDK for shared functionality and re-exports
- `thiserror`: Error handling with proper error types

### Testing Strategy

- Unit tests are inline with modules using `#[cfg(test)]`
- Shared test utilities and tokens are in `src/tests.rs`
- Extension tests require RPC access and must run single-threaded
- Tests match the TypeScript SDK for compatibility verification

### Important Notes

- MSRV (Minimum Supported Rust Version) is 1.86
- When adding new features, maintain compatibility with the TypeScript SDK API
- Use snake_case naming convention (not camelCase like TypeScript)
- Import via prelude for convenience: `use uniswap_v4_sdk::prelude::*`
- Extension tests require `MAINNET_RPC_URL` environment variable for RPC access
