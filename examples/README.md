# Uniswap V4 SDK Examples

This directory contains practical examples demonstrating how to use the Uniswap V4 SDK for Rust to interact with Uniswap
V4 protocols.

## Prerequisites

- Rust 1.88 or later
- A mainnet RPC URL (for forking)

## Setup

1. Create a `.env` file in the project root:

```env
MAINNET_RPC_URL=https://your-ethereum-mainnet-rpc-url
```

2. Build the project with extensions feature:

```bash
cargo build --features extensions
```

## Examples

### Basic Examples

- **[mint_position_basic.rs](./mint_position_basic.rs)** - Demonstrates minting a liquidity position in an existing
  ETH-USDC V4 pool

### Liquidity Management Examples

- **[increase_liquidity.rs](./increase_liquidity.rs)** - Shows how to add more tokens to an existing position,
  demonstrating the complete workflow from initial mint to liquidity increase

### Advanced Examples

- **[mint_position_permit2.rs](./mint_position_permit2.rs)** - Demonstrates using Permit2 for gasless token approvals
  when minting positions

## Running Examples

Each example can be run independently:

```bash
# Run the basic minting example
cargo run --example mint_position_basic --features extensions

# Run the liquidity increase example
cargo run --example increase_liquidity --features extensions

# Run the permit2 example
cargo run --example mint_position_permit2 --features extensions
```

**Note**: Examples require the `extensions` feature for V4 functionality.

## Key Concepts

### Uniswap V4 vs V3 Differences

- **Hooks**: V4 introduces hooks that can customize pool behavior
- **Currencies**: V4 uses `Currency` instead of `Token` to support native ETH
- **Position Manager**: New position manager contract with different interface
- **Pool Keys**: V4 pools are identified by pool keys containing currency pair, fee, tick spacing, and hooks

### Position Minting Process

1. **Create or reference a pool** with the desired currency pair, fee tier, and hooks
2. **Define position parameters** including tick range and liquidity amount
3. **Prepare transaction** using `add_call_parameters()` with appropriate options
4. **Execute transaction** through the V4 Position Manager contract

### Testing Setup

All examples use Anvil forking to create a local testnet that mirrors the mainnet state:

- Fork from a recent mainnet block
- Create test accounts with ETH balances
- Set up token balances and approvals
- Execute transactions in the forked environment

## Common Patterns

- Use `uniswap_v4_sdk::prelude::*` for easy imports
- Use shared utilities from `examples/common` module
- Set up Anvil provider for local testing
- Handle both native ETH and ERC20 tokens as currencies
- Use appropriate slippage tolerance and deadline parameters
