//! ## Position Extension
//! Functions to query and work with Uniswap V4 positions from the position manager NFT.
//!
//! ## Features
//!
//! - **Position fetching**: Query full position data from NFT token IDs
//! - **Event parsing**: Extract position keys and token IDs from transaction events
//!
//! ## Architecture
//!
//! V4 positions have a two-tier architecture:
//! - **Position Manager**: ERC721 NFT contract managing positions via token IDs
//! - **Pool Manager**: Core contract storing actual position state (liquidity, fees)
//!
//! Position keys link these systems: derived from ModifyLiquidity events for pool manager queries,
//! while token IDs from Transfer events identify position manager NFTs.

use crate::{
    entities::{pool::Pool, position::Position},
    prelude::*,
};
use alloc::vec::Vec;
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::Network,
    providers::Provider,
    rpc::types::{Filter, TransactionReceipt},
};
use alloy_primitives::{aliases::I24, Address, ChainId, B256, I256, U256};
use alloy_sol_types::SolEvent;

/// Fetches position data from the position manager NFT and creates a Position.
///
/// ## Arguments
///
/// * `chain_id` - The chain id
/// * `position_manager` - The address of the V4 position manager contract
/// * `token_id` - The NFT token ID of the position
/// * `provider` - The provider instance for blockchain queries
/// * `block_id` - Optional block number to query
///
/// ## Returns
///
/// A [`Position`] struct with pool data and position parameters
#[inline]
pub async fn get_position<N, P>(
    chain_id: ChainId,
    position_manager: Address,
    token_id: U256,
    provider: P,
    block_id: Option<BlockId>,
) -> Result<Position, Error>
where
    N: Network,
    P: Provider<N>,
{
    let block_id = block_id.unwrap_or(BlockId::latest());
    let pm_contract = IPositionManagerView::new(position_manager, &provider);

    // Fetch pool manager address, pool key, position info, and liquidity
    let pool_manager_call = pm_contract.poolManager().block(block_id);
    let pool_and_info_call = pm_contract.getPoolAndPositionInfo(token_id).block(block_id);
    let liquidity_call = pm_contract.getPositionLiquidity(token_id).block(block_id);

    let (pool_manager, pool_and_info, liquidity) = tokio::join!(
        pool_manager_call.call(),
        pool_and_info_call.call(),
        liquidity_call.call()
    );

    let pool_and_info_result = pool_and_info?;
    let pool_key = pool_and_info_result._0;

    // Decode tick_lower and tick_upper from packed position info
    let (tick_lower, tick_upper) = decode_position_info(pool_and_info_result._1);

    // Fetch pool data from pool manager
    let pool = Pool::from_pool_key(
        chain_id,
        pool_manager?,
        pool_key.currency0,
        pool_key.currency1,
        pool_key.fee,
        pool_key.tickSpacing,
        pool_key.hooks,
        provider,
        Some(block_id),
    )
    .await?;

    Ok(Position::new(
        pool,
        liquidity?,
        tick_lower.as_i32(),
        tick_upper.as_i32(),
    ))
}

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

/// Decodes tick_lower and tick_upper from a packed PositionInfo uint256.
///
/// ## PositionInfo Layout (from least significant bit)
///
/// - Bits 0-7: hasSubscriber (8 bits)
/// - Bits 8-31: tickLower (24 bits, signed)
/// - Bits 32-55: tickUpper (24 bits, signed)
/// - Bits 56-255: poolId (200 bits, truncated)
///
/// ## Arguments
///
/// * `position_info` - The packed PositionInfo as a U256
///
/// ## Returns
///
/// A tuple of (tick_lower, tick_upper) as signed 24-bit integers
fn decode_position_info(position_info: U256) -> (I24, I24) {
    let tick_lower = I256::from_raw(position_info << 224).asr(232);
    let tick_upper = I256::from_raw(position_info << 200).asr(232);
    (I24::from(tick_lower), I24::from(tick_upper))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use alloy::rpc::types::Filter;
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::addresses::CHAIN_TO_ADDRESSES_MAP;
    use uniswap_v3_sdk::entities::TickIndex;

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

    /// Helper function to find and fetch a transaction receipt containing NFT minting events
    async fn get_mint_receipt(position_manager: Address) -> TransactionReceipt {
        let filter = Filter::new()
            .from_block(FROM_BLOCK)
            .to_block(TO_BLOCK)
            .event_signature(Transfer::SIGNATURE_HASH)
            .address(position_manager)
            .topic1(B256::ZERO); // from address(0) - minting events

        let logs = PROVIDER.get_logs(&filter).await.unwrap();
        assert!(!logs.is_empty(), "Should find Transfer events");

        let tx_hash = logs.first().unwrap().transaction_hash.unwrap();
        PROVIDER
            .get_transaction_receipt(tx_hash)
            .await
            .unwrap()
            .unwrap()
    }

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
        let receipt = get_mint_receipt(position_manager).await;

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
        let receipt = get_mint_receipt(position_manager).await;

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

    #[tokio::test]
    async fn test_get_position() {
        let position_manager = *V4_POSITION_MANAGER;
        let receipt = get_mint_receipt(position_manager).await;

        let token_id = get_first_token_id_from_transaction(position_manager, &receipt)
            .expect("Should find a token ID");

        // Fetch the position
        let position = get_position(1, position_manager, token_id, PROVIDER.clone(), BLOCK_ID)
            .await
            .unwrap();

        // Verify the position is valid
        assert!(
            position.liquidity > 0,
            "Position should have non-zero liquidity"
        );
        assert!(
            position.tick_lower < position.tick_upper,
            "tick_lower should be less than tick_upper"
        );
        assert!(
            !position.pool.sqrt_price_x96.is_zero(),
            "Pool should have valid price"
        );

        // Validate against pool manager direct query
        // Calculate position key: Position.calculatePositionKey(address(this), tickLower,
        // tickUpper, bytes32(tokenId))
        let position_key = calculate_position_key(
            position_manager,
            position.tick_lower.to_i24(),
            position.tick_upper.to_i24(),
            B256::from(token_id),
        );

        // Query pool manager directly using PoolManagerLens
        let lens = PoolManagerLens::new(*V4_POOL_MANAGER, PROVIDER.clone());
        let liquidity_from_lens = lens
            .get_position_liquidity(position.pool.pool_id, position_key, BLOCK_ID)
            .await
            .unwrap();

        // Verify liquidity matches between position manager view and direct pool manager query
        assert_eq!(
            position.liquidity, liquidity_from_lens,
            "Liquidity should match between position manager view and pool manager direct query"
        );
    }
}
