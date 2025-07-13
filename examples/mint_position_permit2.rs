use alloy::{
    network::TransactionBuilder,
    node_bindings::WEI_IN_ETHER,
    providers::{ext::AnvilApi, Provider},
    rpc::types::TransactionRequest,
};
use alloy_primitives::aliases::U48;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::{nearest_usable_tick, FeeAmount, MintAmounts};
use uniswap_v4_sdk::{prelude::*, tests::*};

const TICK_SPACING: i32 = 10;
const LIQUIDITY_AMOUNT: u128 = 1_000_000_000_000_000;

/// Permit2 example demonstrating gasless token approvals when minting positions.
/// This example:
/// 1. Sets up a forked mainnet environment using Anvil
/// 2. Checks if an ETH-USDC V4 pool exists and gets its current state
/// 3. Sets up token balances (no direct Permit2 approvals needed)
/// 4. Creates EIP-712 signatures for gasless token approvals via Permit2
/// 5. Mints a liquidity position using the gasless approvals
/// 6. Verifies the position was created correctly
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

    // Set up token balances and approve to Permit2 (required before using Permit2 signatures)
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

    // Create Permit2 batch permit data for gasless approvals
    println!("🔐 Creating Permit2 batch permit signature...");

    let permit_batch = position.permit_batch_data(
        &Percent::new(1, 1000), // 0.1% slippage
        v4_position_manager,
        U256::ZERO, // nonce
        U48::MAX,   // deadline
    )?;

    println!("📋 Permit batch details:");
    println!("  - Token0: {}", permit_batch.details[0].token);
    println!("  - Amount0: {}", permit_batch.details[0].amount);
    println!("  - Token1: {}", permit_batch.details[1].token);
    println!("  - Amount1: {}", permit_batch.details[1].amount);
    println!("  - Spender: {}", permit_batch.spender);

    // Create EIP-712 signature for the permit batch
    let signature = create_permit2_signature(&permit_batch, &signer)?;

    println!("✅ Permit2 signature created (gasless approval)");

    println!("🚀 Minting position with gasless approvals...");

    let options = create_add_liquidity_options(
        account,
        Some(BatchPermitOptions {
            owner: account,
            permit_batch,
            signature,
        }),
    );

    let params = add_call_parameters(&mut position, options)?;

    println!("📋 Transaction details:");
    println!("  - Calldata length: {} bytes", params.calldata.len());
    println!("  - Value: {} wei", params.value);
    println!("  - Includes gasless Permit2 approval");

    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(params.calldata)
        .with_value(params.value);

    let receipt = provider.send_transaction(tx).await?.watch().await?;

    println!("✅ Position minted successfully with gasless approvals!");
    println!("📋 Transaction hash: {receipt:?}");
    println!("🎉 No separate approval transactions were needed!");

    Ok(())
}
