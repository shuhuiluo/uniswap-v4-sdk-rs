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
/// and extracts position keys from events that represent new positions (positive liquidityDelta).
///
/// ## Arguments
///
/// * `pool_manager` - The address of the V4 pool manager contract
/// * `tx_receipt` - The transaction receipt containing the logs
///
/// ## Returns
///
/// A vector of position keys (as B256) that were created in the transaction.
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
        .filter(|event| event.liquidityDelta.is_positive())
        .map(|event| {
            calculate_position_key(event.sender, event.tickLower, event.tickUpper, event.salt)
        })
        .collect()
}

/// Extracts position keys from ModifyLiquidity events within a block range.
///
/// This function searches for ModifyLiquidity events in a range of blocks for a specific
/// pool and extracts position keys from events that represent new positions (positive
/// liquidityDelta).
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
/// A vector of position keys (as B256) that were created in the specified block range.
#[inline]
pub async fn get_position_keys_in_blocks<P, N>(
    pool_manager: Address,
    pool_id: B256,
    from_block: BlockNumberOrTag,
    to_block: BlockNumberOrTag,
    provider: P,
) -> Result<Vec<B256>, Error>
where
    P: Provider<N>,
    N: Network,
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
        .filter(|event| event.liquidityDelta.is_positive())
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
    let token_ids = get_token_ids_from_transaction(position_manager, tx_receipt);
    token_ids.first().map(|(_, token_id)| *token_id)
}
