use alloy::{
    network::TransactionBuilder,
    node_bindings::WEI_IN_ETHER,
    providers::{ext::AnvilApi, Provider},
    rpc::types::TransactionRequest,
};
use alloy_primitives::{aliases::U48, U160};
use alloy_sol_types::SolCall;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::{nearest_usable_tick, FeeAmount, MintAmounts};
use uniswap_v4_sdk::{prelude::*, tests::*};

const TICK_SPACING: i32 = 10;
const LIQUIDITY_AMOUNT: u128 = 1_000_000_000_000_000;

/// Basic example demonstrating how to mint a liquidity position in an existing Uniswap V4 pool.
/// This example:
/// 1. Sets up a forked mainnet environment using Anvil
/// 2. Checks if an ETH-USDC V4 pool exists and gets its current state
/// 3. Sets up token balances and Permit2 approvals (required for V4)
/// 4. Mints a liquidity position in the pool
/// 5. Verifies the position was created correctly
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use a recent block for forking
    let block_id = 22808980;

    println!("🔗 Setting up Anvil fork from mainnet...");
    let provider = setup_anvil_fork(block_id).await;
    provider.anvil_auto_impersonate_account(true).await.unwrap();

    println!("👤 Creating test account...");
    let signer = setup_test_account(&provider, WEI_IN_ETHER).await;
    let account = signer.address();
    println!("👤 Created test account: {account}");

    // Get V4 contract addresses
    let chain_addresses = CHAIN_TO_ADDRESSES_MAP.get(&1).unwrap();
    let v4_position_manager = chain_addresses.v4_position_manager.unwrap();
    let v4_pool_manager = chain_addresses.v4_pool_manager.unwrap();

    println!("🔍 Setting up ETH-USDC pool...");
    let pool = create_pool(
        &provider,
        v4_pool_manager,
        ETHER.clone().into(),
        USDC.clone().into(),
        FeeAmount::LOW.into(),
        TICK_SPACING,
        Address::ZERO,
    )
    .await?;

    println!(
        "✅ Pool found: {} / {}",
        pool.currency0.symbol().unwrap(),
        pool.currency1.symbol().unwrap()
    );

    println!("📍 V4 Position Manager: {v4_position_manager}");

    let tick_lower = nearest_usable_tick(pool.tick_current - TICK_SPACING * 10, TICK_SPACING);
    let tick_upper = nearest_usable_tick(pool.tick_current + TICK_SPACING * 10, TICK_SPACING);

    println!("📊 Position parameters:");
    println!("  - Tick range: {tick_lower} to {tick_upper}");
    println!("  - Liquidity: {LIQUIDITY_AMOUNT}");

    let mut position = Position::new(pool, LIQUIDITY_AMOUNT, tick_lower, tick_upper);
    let MintAmounts { amount0, amount1 } = position.mint_amounts()?;

    println!("💰 Required amounts:");
    println!("  - Amount0 (ETH): {amount0}");
    println!("  - Amount1 (USDC): {amount1}");

    // Set up token balances and Permit2 approvals
    setup_token_balance(&provider, USDC.address(), account, amount1, PERMIT2_ADDRESS).await?;
    setup_token_balance(
        &provider,
        ETHER.address(),
        account,
        amount0,
        PERMIT2_ADDRESS,
    )
    .await?;
    println!("💳 Token balances and Permit2 approvals set up");

    // Approve v4_position_manager on Permit2 for USDC transfers (V4 requirement)
    println!("🔐 Approving v4_position_manager on Permit2...");

    let approve_call = IAllowanceTransfer::approveCall {
        token: USDC.address(),
        spender: v4_position_manager,
        amount: U160::from(amount1),
        expiration: U48::MAX,
    };

    let approve_tx = TransactionRequest::default()
        .with_from(account)
        .with_to(PERMIT2_ADDRESS)
        .with_input(approve_call.abi_encode());

    provider.send_transaction(approve_tx).await?.watch().await?;
    println!("✅ Permit2 approval successful");

    println!("🚀 Minting position...");

    let options = create_add_liquidity_options(account, None);

    let params = add_call_parameters(&mut position, options)?;

    println!("📋 Transaction details:");
    println!("  - Calldata length: {} bytes", params.calldata.len());
    println!("  - Value: {} wei", params.value);

    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(params.calldata)
        .with_value(params.value);

    let receipt = provider.send_transaction(tx).await?.watch().await?;

    println!("✅ Position minted successfully!");
    println!("📋 Transaction hash: {receipt:?}");

    // In a real application, parse logs to get token ID and verify position
    Ok(())
}
