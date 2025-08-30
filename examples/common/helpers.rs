//! Common helper functions for examples

use super::{constants::PERMIT2_ADDRESS, tokens::ETHER};
use alloy::{
    network::TransactionBuilder,
    providers::{ext::AnvilApi, Provider},
    rpc::types::TransactionRequest,
    signers::{local::PrivateKeySigner, SignerSync},
    sol_types::{eip712_domain, Eip712Domain, SolStruct},
};
use alloy_primitives::{
    aliases::{U24, U48},
    Address, Bytes, Signature, B256, U160, U256,
};
use alloy_sol_types::SolCall;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;
use uniswap_v4_sdk::{
    entities::Pool,
    extensions::PoolManagerLens,
    position_manager::{AddLiquidityOptions, AddLiquiditySpecificOptions, MintSpecificOptions},
    prelude::{Error, *},
};

/// Create a pool from on-chain state
#[inline]
pub async fn create_pool(
    provider: &impl Provider,
    v4_pool_manager: Address,
    currency0: Currency,
    currency1: Currency,
    fee: U24,
    tick_spacing: i32,
    hook_address: Address,
) -> Result<Pool, Error> {
    let pool_id = Pool::get_pool_id(&currency0, &currency1, fee, tick_spacing, hook_address)?;

    let pool_lens = PoolManagerLens::new(v4_pool_manager, provider);
    let (actual_sqrt_price, _, _, _) = pool_lens.get_slot0(pool_id, None).await?;

    Pool::new(
        currency0,
        currency1,
        fee,
        tick_spacing,
        hook_address,
        actual_sqrt_price,
        0,
    )
}

/// Setup token balance and approval for a single token
#[inline]
pub async fn setup_token_balance(
    provider: &impl Provider,
    token_address: Address,
    account: Address,
    amount: U256,
    approve_to: Address,
) -> Result<(), Error> {
    let overrides =
        get_erc20_state_overrides(token_address, account, approve_to, amount, provider).await?;

    for (token, account_override) in overrides {
        for (slot, value) in account_override.state_diff.unwrap() {
            provider
                .anvil_set_storage_at(token, U256::from_be_bytes(slot.0), value)
                .await
                .map_err(|e| Error::ContractError(e.into()))?;
        }
    }

    Ok(())
}

/// Helper function to set up token balance and Permit2 approval for a specific token
#[inline]
pub async fn setup_token_and_approval(
    provider: &impl Provider,
    account: Address,
    v4_position_manager: Address,
    token_address: Address,
    amount: U256,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up token balance
    setup_token_balance(provider, token_address, account, amount, PERMIT2_ADDRESS).await?;

    // Approve v4_position_manager on Permit2 for token transfers
    let approve_call = IAllowanceTransfer::approveCall {
        token: token_address,
        spender: v4_position_manager,
        amount: U160::from(amount),
        expiration: U48::MAX,
    };

    let approve_tx = TransactionRequest::default()
        .with_from(account)
        .with_to(PERMIT2_ADDRESS)
        .with_input(approve_call.abi_encode());

    provider.send_transaction(approve_tx).await?.watch().await?;
    Ok(())
}

/// Get Permit2 EIP-712 domain for mainnet
#[inline]
#[must_use]
pub const fn get_permit2_domain() -> Eip712Domain {
    eip712_domain! {
        name: "Permit2",
        chain_id: 1,
        verifying_contract: PERMIT2_ADDRESS,
    }
}

/// Create EIP-712 signature for Permit2 batch permit
#[inline]
pub fn create_permit2_signature(
    permit_batch: &AllowanceTransferPermitBatch,
    signer: &PrivateKeySigner,
) -> Bytes {
    let domain = get_permit2_domain();
    let hash: B256 = permit_batch.eip712_signing_hash(&domain);
    let signature: Signature = signer.sign_hash_sync(&hash).unwrap();
    signature.as_bytes().into()
}

/// Create AddLiquidityOptions for minting positions
#[inline]
pub fn create_add_liquidity_options(
    recipient: Address,
    batch_permit: Option<BatchPermitOptions>,
) -> AddLiquidityOptions {
    AddLiquidityOptions {
        common_opts: CommonOptions {
            slippage_tolerance: Percent::new(1, 1000),
            deadline: U256::MAX,
            hook_data: Default::default(),
        },
        use_native: Some(ETHER.clone()),
        batch_permit,
        specific_opts: AddLiquiditySpecificOptions::Mint(MintSpecificOptions {
            recipient,
            create_pool: false,
            sqrt_price_x96: None,
            migrate: false,
        }),
    }
}
