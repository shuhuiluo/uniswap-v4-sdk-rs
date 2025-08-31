//! Position utilities for parsing identifiers from V4 transactions and events.
//!
//! This module provides functionality to extract both position keys and NFT token IDs from Uniswap
//! V4 transactions. Position keys are derived from ModifyLiquidity events and used for pool manager
//! queries, while NFT token IDs come from ERC721 Transfer events and are used for position manager
//! operations.

use crate::prelude::{calculate_position_key, Error, ModifyLiquidity, Transfer};
use alloc::vec::Vec;
use alloy::{
    eips::BlockNumberOrTag,
    network::Network,
    providers::Provider,
    rpc::types::{Filter, TransactionReceipt},
};
use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::SolEvent;

/// Extracts position keys from ModifyLiquidity events in a specific transaction.
///
/// This function looks for ModifyLiquidity events in the given transaction receipt
/// and extracts position keys from all ModifyLiquidity events.
///
/// ## Arguments
///
/// * `pool_manager` - The address of the V4 pool manager contract
/// * `tx_receipt` - The transaction receipt containing the logs
///
/// ## Returns
///
/// A vector of position keys (as B256) from all ModifyLiquidity events in the transaction.
#[inline]
#[must_use]
pub fn get_position_keys_from_transaction(
    pool_manager: Address,
    tx_receipt: &TransactionReceipt,
) -> Vec<B256> {
    tx_receipt
        .logs()
        .iter()
        .filter(|&log| log.address() == pool_manager)
        .filter(|&log| matches!(log.topic0(), Some(t) if t == &ModifyLiquidity::SIGNATURE_HASH))
        .filter_map(|log| ModifyLiquidity::decode_log_data(log.data()).ok())
        .map(|event| {
            calculate_position_key(event.sender, event.tickLower, event.tickUpper, event.salt)
        })
        .collect()
}

/// Extracts position keys from ModifyLiquidity events within a block range.
///
/// This function searches for ModifyLiquidity events in a range of blocks for a specific
/// pool and extracts position keys from all ModifyLiquidity events.
///
/// ## Arguments
///
/// * `pool_manager` - The address of the V4 pool manager contract
/// * `pool_id` - The ID of the pool to filter events for
/// * `from_block` - The starting block for the search
/// * `to_block` - The ending block for the search
/// * `provider` - The provider instance for blockchain queries
///
/// ## Returns
///
/// A vector of position keys (as B256) from all ModifyLiquidity events in the specified block
/// range.
#[inline]
pub async fn get_position_keys_in_blocks<P, N, T>(
    pool_manager: Address,
    pool_id: B256,
    from_block: T,
    to_block: T,
    provider: P,
) -> Result<Vec<B256>, Error>
where
    P: Provider<N>,
    N: Network,
    T: Into<BlockNumberOrTag>,
{
    let filter = Filter::new()
        .from_block(from_block)
        .to_block(to_block)
        .event_signature(ModifyLiquidity::SIGNATURE_HASH)
        .address(pool_manager)
        .topic1(pool_id);

    let logs = provider
        .get_logs(&filter)
        .await
        .map_err(|e| Error::ContractError(e.into()))?;

    let position_keys: Vec<B256> = logs
        .iter()
        .filter_map(|log| ModifyLiquidity::decode_log_data(log.data()).ok())
        .map(|event| {
            calculate_position_key(event.sender, event.tickLower, event.tickUpper, event.salt)
        })
        .collect();

    Ok(position_keys)
}

/// Extracts all NFT token IDs and their recipients from a position manager transaction.
///
/// This function looks for ERC721 Transfer events in the given transaction receipt
/// from the position manager contract, specifically minting events (from address(0)).
/// Returns all minted token IDs with their respective recipients.
///
/// ## Arguments
///
/// * `position_manager` - The address of the V4 position manager contract
/// * `tx_receipt` - The transaction receipt containing the logs
///
/// ## Returns
///
/// A vector of (recipient, token_id) pairs for all NFTs minted in the transaction.
#[inline]
#[must_use]
pub fn get_token_ids_from_transaction(
    position_manager: Address,
    tx_receipt: &TransactionReceipt,
) -> Vec<(Address, U256)> {
    tx_receipt
        .logs()
        .iter()
        .filter(|&log| log.address() == position_manager)
        .filter(|&log| matches!(log.topic0(), Some(t) if t == &Transfer::SIGNATURE_HASH))
        .filter_map(|log| Transfer::decode_log_data(log.data()).ok())
        .filter(|event| event.from.is_zero()) // Minting: from == address(0)
        .map(|event| (event.to, event.tokenId))
        .collect()
}

/// Extracts the first NFT token ID from a position manager mint transaction.
///
/// This is a convenience function that returns only the first minted token ID,
/// maintaining backward compatibility with code that expects a single token ID.
///
/// ## Arguments
///
/// * `position_manager` - The address of the V4 position manager contract
/// * `tx_receipt` - The transaction receipt containing the logs
///
/// ## Returns
///
/// An optional NFT token ID (as U256) for the first token minted in the transaction.
#[inline]
#[must_use]
pub fn get_first_token_id_from_transaction(
    position_manager: Address,
    tx_receipt: &TransactionReceipt,
) -> Option<U256> {
    tx_receipt
        .logs()
        .iter()
        .filter(|&log| log.address() == position_manager)
        .filter(|&log| matches!(log.topic0(), Some(t) if t == &Transfer::SIGNATURE_HASH))
        .find_map(|log| Transfer::decode_log_data(log.data()).ok())
        .filter(|event| event.from.is_zero()) // Minting: from == address(0)
        .map(|event| event.tokenId)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use alloy::rpc::types::Filter;
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::addresses::CHAIN_TO_ADDRESSES_MAP;

    static V4_POOL_MANAGER: Lazy<Address> = Lazy::new(|| {
        CHAIN_TO_ADDRESSES_MAP
            .get(&1)
            .unwrap()
            .v4_pool_manager
            .unwrap()
    });

    static V4_POSITION_MANAGER: Lazy<Address> = Lazy::new(|| {
        CHAIN_TO_ADDRESSES_MAP
            .get(&1)
            .unwrap()
            .v4_position_manager
            .unwrap()
    });

    const FROM_BLOCK: u64 = BLOCK_ID.unwrap().as_u64().unwrap() - 499;
    const TO_BLOCK: u64 = BLOCK_ID.unwrap().as_u64().unwrap();

    #[tokio::test]
    async fn test_get_position_keys_in_blocks() {
        let position_keys = get_position_keys_in_blocks(
            *V4_POOL_MANAGER,
            *POOL_ID_ETH_USDC,
            FROM_BLOCK,
            TO_BLOCK,
            &*PROVIDER,
        )
        .await
        .unwrap();

        // Should find at least some position keys in this block range
        assert!(
            !position_keys.is_empty(),
            "Should find position keys in block range"
        );

        // All position keys should be valid B256 values (not zero)
        for position_key in &position_keys {
            assert_ne!(*position_key, B256::ZERO, "Position key should not be zero");
        }

        println!("Found {} position keys in block range", position_keys.len());
    }

    #[tokio::test]
    async fn test_get_position_keys_from_transaction() {
        let pool_manager = *V4_POOL_MANAGER;

        // Query for ModifyLiquidity events in our test block range
        let filter = Filter::new()
            .from_block(FROM_BLOCK)
            .to_block(TO_BLOCK)
            .event_signature(ModifyLiquidity::SIGNATURE_HASH)
            .address(pool_manager)
            .topic1(*POOL_ID_ETH_USDC);

        let logs = PROVIDER.get_logs(&filter).await.unwrap();
        assert!(!logs.is_empty(), "Should find ModifyLiquidity events");

        // Get a transaction hash from one of the logs
        let tx_hash = logs.first().unwrap().transaction_hash.unwrap();
        let receipt = PROVIDER
            .get_transaction_receipt(tx_hash)
            .await
            .unwrap()
            .unwrap();

        let position_keys = get_position_keys_from_transaction(pool_manager, &receipt);

        // Verify the position keys are valid
        for position_key in &position_keys {
            assert_ne!(*position_key, B256::ZERO, "Position key should not be zero");
        }
        println!("Found {} position keys in transaction", position_keys.len());
    }

    #[tokio::test]
    async fn test_get_token_ids_from_transaction() {
        let position_manager = *V4_POSITION_MANAGER;

        // Query for Transfer events (minting) in our test block range
        let filter = Filter::new()
            .from_block(FROM_BLOCK)
            .to_block(TO_BLOCK)
            .event_signature(Transfer::SIGNATURE_HASH)
            .address(position_manager)
            .topic1(B256::ZERO); // from address(0) - minting events

        let logs = PROVIDER.get_logs(&filter).await.unwrap();

        // Get a transaction that contains NFT minting
        let tx_hash = logs.first().unwrap().transaction_hash.unwrap();
        let receipt = PROVIDER
            .get_transaction_receipt(tx_hash)
            .await
            .unwrap()
            .unwrap();

        let token_ids = get_token_ids_from_transaction(position_manager, &receipt);

        assert!(
            !token_ids.is_empty(),
            "Should extract token IDs from minting transaction"
        );

        for (recipient, token_id) in token_ids {
            assert_ne!(
                recipient,
                Address::ZERO,
                "Recipient should not be zero address"
            );
            assert_ne!(token_id, U256::ZERO, "Token ID should not be zero");
        }
    }

    #[tokio::test]
    async fn test_get_first_token_id_from_transaction() {
        let position_manager = *V4_POSITION_MANAGER;

        // Query for Transfer events (minting) in our test block range
        let filter = Filter::new()
            .from_block(FROM_BLOCK)
            .to_block(TO_BLOCK)
            .event_signature(Transfer::SIGNATURE_HASH)
            .address(position_manager)
            .topic1(B256::ZERO); // from address(0) - minting events

        let logs = PROVIDER.get_logs(&filter).await.unwrap();

        // Get a transaction that contains NFT minting
        let tx_hash = logs.first().unwrap().transaction_hash.unwrap();
        let receipt = PROVIDER
            .get_transaction_receipt(tx_hash)
            .await
            .unwrap()
            .unwrap();

        let first_token_id = get_first_token_id_from_transaction(position_manager, &receipt);

        assert!(first_token_id.is_some(), "Should return first token ID");
        assert_ne!(
            first_token_id.unwrap(),
            U256::ZERO,
            "First token ID should not be zero"
        );

        // Compare with get_token_ids_from_transaction to ensure consistency
        let all_token_ids = get_token_ids_from_transaction(position_manager, &receipt);
        assert_eq!(
            first_token_id.unwrap(),
            all_token_ids.first().unwrap().1,
            "First token ID should match"
        );
    }
}
