//! Common provider setup utilities for examples

use super::constants::RPC_URL;
use alloy::{
    providers::{ext::AnvilApi, Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
};
use alloy_primitives::U256;

/// Set up an Anvil fork from mainnet at a specific block
#[inline]
pub fn setup_anvil_fork(fork_block: u64) -> impl Provider + Clone {
    ProviderBuilder::new().connect_anvil_with_config(|anvil| {
        anvil.fork(RPC_URL.clone()).fork_block_number(fork_block)
    })
}

/// Create a test account with ETH balance
#[inline]
pub async fn setup_test_account(provider: &impl Provider, balance: U256) -> PrivateKeySigner {
    let signer = PrivateKeySigner::random();
    let account = signer.address();
    provider.anvil_set_balance(account, balance).await.unwrap();
    signer
}
