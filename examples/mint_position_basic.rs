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
const LIQUIDITY_AMOUNT: u128 = 1_000_000_000_000_000;

/// Basic example demonstrating how to mint a liquidity position in an existing Uniswap V4 pool
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use a recent block for forking
    let block_id = 22808980;

    let provider = setup_anvil_fork(block_id);
    provider.anvil_auto_impersonate_account(true).await.unwrap();
    let signer = setup_test_account(&provider, WEI_IN_ETHER).await;
    let account = signer.address();

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

    let mut position = Position::new(pool, LIQUIDITY_AMOUNT, tick_lower, tick_upper);
    let MintAmounts { amount1, .. } = position.mint_amounts()?;

    // Set up USDC balance and Permit2 approval
    setup_token_and_approval(
        &provider,
        account,
        v4_position_manager,
        USDC.address(),
        amount1,
    )
    .await?;

    let options = create_add_liquidity_options(account, None);
    let params = add_call_parameters(&mut position, options)?;

    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(params.calldata)
        .with_value(params.value);

    let tx_hash = provider.send_transaction(tx).await?.watch().await?;
    let receipt = provider.get_transaction_receipt(tx_hash).await?.unwrap();
    let token_id = get_first_token_id_from_transaction(v4_position_manager, &receipt).unwrap();
    println!("Position minted with token ID: {token_id}");

    Ok(())
}
