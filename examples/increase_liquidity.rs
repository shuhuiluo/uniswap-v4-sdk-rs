use alloy::{
    network::TransactionBuilder,
    node_bindings::WEI_IN_ETHER,
    providers::{ext::AnvilApi, Provider},
    rpc::types::TransactionRequest,
};
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::{nearest_usable_tick, FeeAmount, MintAmounts};
use uniswap_v4_sdk::{extensions::get_first_token_id_from_transaction, prelude::*};

#[path = "common/mod.rs"]
mod common;
use common::*;

const TICK_SPACING: i32 = 10;
const INITIAL_LIQUIDITY: u128 = 500_000_000_000_000;
const ADDITIONAL_LIQUIDITY: u128 = 300_000_000_000_000;

/// Example demonstrating how to increase liquidity for an existing position
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use a recent block for forking
    let block_id = 22808980;

    let provider = setup_anvil_fork(block_id);
    provider.anvil_auto_impersonate_account(true).await.unwrap();
    let signer = setup_test_account(&provider, WEI_IN_ETHER).await;
    let account = signer.address();

    // Get V4 contract addresses
    let v4_position_manager = *V4_POSITION_MANAGER;
    let v4_pool_manager = *V4_POOL_MANAGER;

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

    let tick_lower = nearest_usable_tick(pool.tick_current - TICK_SPACING * 10, TICK_SPACING);
    let tick_upper = nearest_usable_tick(pool.tick_current + TICK_SPACING * 10, TICK_SPACING);

    // STEP 1: MINT INITIAL POSITION

    let mut initial_position =
        Position::new(pool.clone(), INITIAL_LIQUIDITY, tick_lower, tick_upper);
    let MintAmounts {
        amount1: initial_amount1,
        ..
    } = initial_position.mint_amounts()?;

    // Set up USDC balance with approval for initial mint
    setup_token_and_approval(
        &provider,
        account,
        v4_position_manager,
        USDC.address(),
        initial_amount1,
    )
    .await?;

    let initial_options = create_add_liquidity_options(account, None);
    let initial_params = add_call_parameters(&mut initial_position, initial_options)?;

    let initial_tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(initial_params.calldata)
        .with_value(initial_params.value);

    let initial_tx_hash = provider.send_transaction(initial_tx).await?.watch().await?;

    // Get the full transaction receipt to parse logs
    let receipt = provider
        .get_transaction_receipt(initial_tx_hash)
        .await?
        .unwrap();

    // Parse the NFT token ID from transaction logs
    let token_id = get_first_token_id_from_transaction(v4_position_manager, &receipt).unwrap();

    // STEP 2: INCREASE LIQUIDITY

    let mut additional_position =
        Position::new(pool.clone(), ADDITIONAL_LIQUIDITY, tick_lower, tick_upper);
    let MintAmounts {
        amount1: additional_amount1,
        ..
    } = additional_position.mint_amounts()?;

    // Set up additional USDC balance with approval for increase
    setup_token_and_approval(
        &provider,
        account,
        v4_position_manager,
        USDC.address(),
        additional_amount1,
    )
    .await?;

    let increase_options = create_increase_liquidity_options(token_id, None);
    let increase_params = add_call_parameters(&mut additional_position, increase_options)?;

    let increase_tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(increase_params.calldata)
        .with_value(increase_params.value);

    provider
        .send_transaction(increase_tx)
        .await?
        .watch()
        .await?;

    let total_liquidity = INITIAL_LIQUIDITY + ADDITIONAL_LIQUIDITY;
    println!(
        "Position {token_id}: liquidity increased from {INITIAL_LIQUIDITY} to {total_liquidity}"
    );

    Ok(())
}
