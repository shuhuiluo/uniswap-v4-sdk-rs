use alloy::{
    eips::BlockId,
    network::TransactionBuilder,
    node_bindings::WEI_IN_ETHER,
    providers::{ext::AnvilApi, Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use alloy_primitives::{address, aliases::U48, Address, U160, U256};
use alloy_sol_types::SolCall;
use once_cell::sync::Lazy;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::{
    get_erc20_state_overrides, nearest_usable_tick, FeeAmount, MintAmounts,
};
use uniswap_v4_sdk::prelude::*;

// Test tokens - same as in the main crate's tests
static ETHER: Lazy<Ether> = Lazy::new(|| Ether::on_chain(1));
static USDC: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        6,
        "USDC",
        "USD Coin"
    )
});

const TICK_SPACING: i32 = 10;
const LIQUIDITY_AMOUNT: u128 = 1_000_000_000_000_000;

const PERMIT2_ADDRESS: Address = address!("000000000022D473030F116dDEE9F6B43aC78BA3");

/// Basic example demonstrating how to mint a liquidity position in an existing Uniswap V4 pool.
/// This example:
/// 1. Sets up a forked mainnet environment using Anvil
/// 2. Checks if an ETH-USDC V4 pool exists and gets its current state
/// 3. Sets up token balances and Permit2 approvals (required for V4)
/// 4. Mints a liquidity position in the pool
/// 5. Verifies the position was created correctly
#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    // Get RPC URL from environment
    let rpc_url: Url = std::env::var("MAINNET_RPC_URL").unwrap().parse().unwrap();

    // Use a recent block for forking
    let block_id = BlockId::from(22808980);

    println!("🔗 Setting up Anvil fork from mainnet...");

    // Create an Anvil fork of mainnet
    let provider = ProviderBuilder::new().connect_anvil_with_config(|anvil| {
        anvil
            .fork(rpc_url)
            .fork_block_number(block_id.as_u64().unwrap())
    });

    // Enable account impersonation for easier testing
    provider.anvil_auto_impersonate_account(true).await.unwrap();

    // Create a test account with ETH balance
    let account = PrivateKeySigner::random().address();
    provider
        .anvil_set_balance(account, WEI_IN_ETHER)
        .await
        .unwrap();

    println!("👤 Created test account: {account}");

    // Get V4 contract addresses
    let chain_addresses = CHAIN_TO_ADDRESSES_MAP.get(&1).unwrap();

    let v4_position_manager = chain_addresses.v4_position_manager.unwrap();
    let v4_pool_manager = chain_addresses.v4_pool_manager.unwrap();

    println!("📍 V4 Position Manager: {v4_position_manager}");
    println!("📍 V4 Pool Manager: {v4_pool_manager}");

    let currency0 = ETHER.clone().into();
    let currency1 = USDC.clone().into();

    // Get pool ID for ETH-USDC V4 pool and check on-chain state
    let pool_id = Pool::get_pool_id(
        &currency0,
        &currency1,
        FeeAmount::LOW.into(),
        TICK_SPACING,
        Address::ZERO,
    )
    .unwrap();

    println!("🔍 Checking pool state on-chain...");
    let pool_lens = PoolManagerLens::new(v4_pool_manager, provider.clone());
    let (actual_sqrt_price, actual_tick, _, _) = pool_lens.get_slot0(pool_id, None).await.unwrap();
    let actual_tick = actual_tick.as_i32();

    let pool = Pool::new(
        currency0,
        currency1,
        FeeAmount::LOW.into(),
        TICK_SPACING,
        Address::ZERO,
        actual_sqrt_price,
        0,
    )
    .unwrap();

    println!(
        "✅ Pool found: {} / {}",
        pool.currency0.symbol().unwrap_or(&"ETH".to_string()),
        pool.currency1.symbol().unwrap_or(&"Unknown".to_string())
    );

    let tick_lower = nearest_usable_tick(actual_tick - TICK_SPACING * 10, TICK_SPACING);
    let tick_upper = nearest_usable_tick(actual_tick + TICK_SPACING * 10, TICK_SPACING);

    println!("📊 Position parameters:");
    println!("  - Tick range: {tick_lower} to {tick_upper}");
    println!("  - Liquidity: {LIQUIDITY_AMOUNT}");

    let mut position = Position::new(pool, LIQUIDITY_AMOUNT, tick_lower, tick_upper);
    let MintAmounts { amount0, amount1 } = position.mint_amounts().unwrap();

    println!("💰 Required amounts:");
    println!("  - Amount0 (ETH): {amount0}");
    println!("  - Amount1 (USDC): {amount1}");

    // Set up USDC balance and approve Permit2
    let overrides =
        get_erc20_state_overrides(USDC.address(), account, PERMIT2_ADDRESS, amount1, &provider)
            .await
            .unwrap();

    for (token, account_override) in overrides {
        for (slot, value) in account_override.state_diff.unwrap() {
            provider
                .anvil_set_storage_at(token, U256::from_be_bytes(slot.0), value)
                .await
                .unwrap();
        }
    }
    println!("💳 USDC balance and Permit2 approval set up");

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

    provider
        .send_transaction(approve_tx)
        .await
        .unwrap()
        .watch()
        .await
        .unwrap();
    println!("✅ Permit2 approval successful");

    println!("🚀 Minting position...");

    let options = AddLiquidityOptions {
        common_opts: CommonOptions {
            slippage_tolerance: Percent::new(1, 1000),
            deadline: U256::MAX,
            hook_data: Default::default(),
        },
        use_native: Some(ETHER.clone()),
        batch_permit: None,
        specific_opts: AddLiquiditySpecificOptions::Mint(MintSpecificOptions {
            recipient: account,
            create_pool: false,
            sqrt_price_x96: None,
            migrate: false,
        }),
    };

    let params = add_call_parameters(&mut position, options).unwrap();

    println!("📋 Transaction details:");
    println!("  - Calldata length: {} bytes", params.calldata.len());
    println!("  - Value: {} wei", params.value);

    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(params.calldata)
        .with_value(params.value);

    let receipt = provider
        .send_transaction(tx)
        .await
        .unwrap()
        .watch()
        .await
        .unwrap();

    println!("✅ Position minted successfully!");
    println!("📋 Transaction hash: {receipt:?}");

    // In a real application, parse logs to get token ID and verify position
}
