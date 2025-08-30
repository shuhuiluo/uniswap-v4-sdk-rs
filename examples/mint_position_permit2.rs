use alloy::{
    network::TransactionBuilder,
    node_bindings::WEI_IN_ETHER,
    providers::{ext::AnvilApi, Provider},
    rpc::types::TransactionRequest,
};
use alloy_primitives::aliases::U48;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::{nearest_usable_tick, FeeAmount, MintAmounts};
use uniswap_v4_sdk::{extensions::get_first_token_id_from_transaction, prelude::*};

#[path = "common/mod.rs"]
mod common;
use common::*;

const TICK_SPACING: i32 = 10;
const LIQUIDITY_AMOUNT: u128 = 1_000_000_000_000_000;

/// Example demonstrating gasless token approvals with Permit2 when minting positions
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
    let MintAmounts { amount0, amount1 } = position.mint_amounts()?;

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

    let permit_batch = position.permit_batch_data(
        &Percent::new(1, 1000), // 0.1% slippage
        v4_position_manager,
        U256::ZERO, // nonce
        U48::MAX,   // deadline
    )?;

    let signature = create_permit2_signature(&permit_batch, &signer);

    let options = create_add_liquidity_options(
        account,
        Some(BatchPermitOptions {
            owner: account,
            permit_batch,
            signature,
        }),
    );

    let params = add_call_parameters(&mut position, options)?;

    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(v4_position_manager)
        .with_input(params.calldata)
        .with_value(params.value);

    let tx_hash = provider.send_transaction(tx).await?.watch().await?;
    let receipt = provider.get_transaction_receipt(tx_hash).await?.unwrap();
    let token_id = get_first_token_id_from_transaction(v4_position_manager, &receipt).unwrap();
    println!("Position minted with gasless approvals, token ID: {token_id}");

    Ok(())
}
