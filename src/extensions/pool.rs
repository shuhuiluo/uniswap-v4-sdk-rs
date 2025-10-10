//! ## Pool Extension
//! This module provides functions to create a V4 [`Pool`] struct from pool parameters by fetching
//! on-chain data including pool state and token metadata.

use crate::{entities::pool::Pool as V4Pool, extensions::PoolManagerLens, prelude::*};
use alloc::string::String;
use alloy::{eips::BlockId, network::Network, providers::Provider};
use alloy_primitives::{aliases::U24, Address, ChainId};
use uniswap_sdk_core::{
    error::Error as CoreError,
    prelude::{Currency, Ether, Token},
};
use uniswap_v3_sdk::{
    entities::TickIndex, extensions::lens::bindings::ierc20metadata::IERC20Metadata,
};

impl V4Pool {
    /// Get a V4 [`Pool`] struct from pool parameters
    ///
    /// ## Arguments
    ///
    /// * `chain_id`: The chain id
    /// * `manager`: The pool manager address
    /// * `currency_a`: Address of one currency in the pool (Address::ZERO for native ETH)
    /// * `currency_b`: Address of the other currency in the pool (Address::ZERO for native ETH)
    /// * `fee`: Fee tier of the pool
    /// * `tick_spacing`: Tick spacing of the pool
    /// * `hooks`: Hook contract address
    /// * `provider`: The alloy provider
    /// * `block_id`: Optional block number to query
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub async fn from_pool_key<I, N, P>(
        chain_id: ChainId,
        manager: Address,
        currency_a: Address,
        currency_b: Address,
        fee: U24,
        tick_spacing: I,
        hooks: Address,
        provider: P,
        block_id: Option<BlockId>,
    ) -> Result<Self, Error>
    where
        I: TickIndex,
        N: Network,
        P: Provider<N> + Clone,
    {
        let block_id = block_id.unwrap_or(BlockId::latest());

        // Create temporary currencies for pool key calculation
        let temp_currency_a = if currency_a.is_zero() {
            Currency::NativeCurrency(Ether::on_chain(chain_id))
        } else {
            Currency::Token(Token::new(chain_id, currency_a, 18, None, None, 0, 0))
        };

        let temp_currency_b = if currency_b.is_zero() {
            Currency::NativeCurrency(Ether::on_chain(chain_id))
        } else {
            Currency::Token(Token::new(chain_id, currency_b, 18, None, None, 0, 0))
        };

        // Calculate pool ID (needs I24 for the generic parameter)
        let pool_id =
            Self::get_pool_id(&temp_currency_a, &temp_currency_b, fee, tick_spacing, hooks)?;

        // Create pool manager lens for querying state
        let lens = PoolManagerLens::new(manager, provider.clone());

        // Get pool state (slot0 contains sqrt_price_x96, tick, protocol_fee, lp_fee)
        let (sqrt_price_x96, _, _, _) = lens.get_slot0(pool_id, Some(block_id)).await?;

        // Validate pool is initialized
        assert!(
            !sqrt_price_x96.is_zero(),
            "Pool has been created but not yet initialized"
        );

        // Get liquidity from the pool
        let liquidity = lens.get_liquidity(pool_id, Some(block_id)).await?;

        // Fetch token metadata separately for cleaner code
        let final_currency_a = if currency_a.is_zero() {
            Currency::NativeCurrency(Ether::on_chain(chain_id))
        } else {
            let token_a_contract = IERC20Metadata::new(currency_a, provider.root());
            let multicall_a = provider
                .multicall()
                .add(token_a_contract.decimals())
                .add(token_a_contract.name())
                .add(token_a_contract.symbol());
            let (decimals, name, symbol): (u8, String, String) = multicall_a
                .block(block_id)
                .aggregate()
                .await
                .map_err(|_| Error::Core(CoreError::Invalid("Token metadata fetch failed")))?;

            Currency::Token(Token::new(
                chain_id,
                currency_a,
                decimals,
                Some(symbol),
                Some(name),
                0,
                0,
            ))
        };

        let final_currency_b = if currency_b.is_zero() {
            Currency::NativeCurrency(Ether::on_chain(chain_id))
        } else {
            let token_b_contract = IERC20Metadata::new(currency_b, provider.root());
            let multicall_b = provider
                .multicall()
                .add(token_b_contract.decimals())
                .add(token_b_contract.name())
                .add(token_b_contract.symbol());
            let (decimals, name, symbol): (u8, String, String) = multicall_b
                .block(block_id)
                .aggregate()
                .await
                .map_err(|_| Error::Core(CoreError::Invalid("Token metadata fetch failed")))?;

            Currency::Token(Token::new(
                chain_id,
                currency_b,
                decimals,
                Some(symbol),
                Some(name),
                0,
                0,
            ))
        };

        Self::new(
            final_currency_a,
            final_currency_b,
            fee,
            tick_spacing.to_i24().as_i32(),
            hooks,
            sqrt_price_x96,
            liquidity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    const TICK_SPACING: i32 = 10;

    #[tokio::test]
    async fn test_from_pool_key() {
        // Use the ETH-USDC pool with fee 500 (0.05%) and tick spacing 10
        let pool = V4Pool::from_pool_key(
            1,
            *POOL_MANAGER_ADDRESS,
            Address::ZERO, // ETH
            USDC.address,
            U24::from(500),
            TICK_SPACING,
            Address::ZERO, // No hooks
            PROVIDER.clone(),
            BLOCK_ID,
        )
        .await
        .unwrap();

        // Verify pool was created successfully
        assert!(
            !pool.sqrt_price_x96.is_zero(),
            "sqrt_price_x96 should be non-zero"
        );
        assert_ne!(
            pool.liquidity, 0,
            "liquidity should be non-zero for active pool"
        );

        // Verify currency0 is ETH (native currency)
        assert!(
            matches!(pool.currency0, Currency::NativeCurrency(_)),
            "currency0 should be native ETH"
        );

        // Verify currency1 is USDC token with correct metadata
        if let Currency::Token(token) = &pool.currency1 {
            assert_eq!(token.decimals, 6, "USDC should have 6 decimals");
            assert_eq!(
                token.symbol.as_deref(),
                Some("USDC"),
                "USDC symbol should match"
            );
        } else {
            panic!("currency1 should be a Token");
        }

        // Verify pool parameters
        assert_eq!(pool.fee, U24::from(500), "fee should be 500");
        assert_eq!(pool.tick_spacing, TICK_SPACING, "tick_spacing should be 10");
        assert_eq!(pool.hooks, Address::ZERO, "hooks should be zero address");
    }
}
