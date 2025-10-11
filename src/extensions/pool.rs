//! ## Pool Extension
//! This module provides functions to create a V4 [`Pool`] struct from pool parameters by fetching
//! on-chain data including pool state and token metadata.

use crate::{entities::pool::Pool, prelude::*};
use alloc::string::{String, ToString};
use alloy::{eips::BlockId, network::Network, providers::Provider};
use alloy_primitives::{aliases::U24, Address, ChainId};
use uniswap_sdk_core::{
    prelude::{Currency, Ether, Token},
    token,
};
use uniswap_v3_sdk::{
    entities::TickIndex, extensions::lens::bindings::ierc20metadata::IERC20Metadata,
};

impl Pool {
    /// Get a V4 [`Pool`] struct from pool parameters
    ///
    /// Fetches pool state and token metadata in parallel using `tokio::join!`.
    /// When using [`CallBatchLayer`](https://docs.rs/alloy-provider/latest/alloy_provider/layers/struct.CallBatchLayer.html),
    /// parallel calls are automatically batched (only for latest block queries).
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
    pub async fn from_pool_key<P, N, I>(
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
        P: Provider<N>,
        N: Network,
        I: TickIndex,
    {
        let block_id = block_id.unwrap_or(BlockId::latest());

        // Create temporary currencies for pool key calculation (without metadata)
        let temp_currency_a = create_currency(chain_id, currency_a, None);
        let temp_currency_b = create_currency(chain_id, currency_b, None);

        let pool_id =
            Self::get_pool_id(&temp_currency_a, &temp_currency_b, fee, tick_spacing, hooks)?;

        let lens = PoolManagerLens::new(manager, &provider);

        let (slot0, liquidity, token_a_data, token_b_data) = tokio::join!(
            lens.get_slot0(pool_id, Some(block_id)),
            lens.get_liquidity(pool_id, Some(block_id)),
            async {
                if currency_a.is_zero() {
                    Ok(None)
                } else {
                    fetch_token_metadata::<N, _>(currency_a, &provider, block_id)
                        .await
                        .map(Some)
                }
            },
            async {
                if currency_b.is_zero() {
                    Ok(None)
                } else {
                    fetch_token_metadata::<N, _>(currency_b, &provider, block_id)
                        .await
                        .map(Some)
                }
            }
        );

        let (sqrt_price_x96, _, _, _) = slot0?;
        let liquidity = liquidity?;

        assert!(
            !sqrt_price_x96.is_zero(),
            "Pool has been created but not yet initialized"
        );

        Self::new(
            create_currency(chain_id, currency_a, token_a_data?),
            create_currency(chain_id, currency_b, token_b_data?),
            fee,
            tick_spacing.to_i24().as_i32(),
            hooks,
            sqrt_price_x96,
            liquidity,
        )
    }
}

impl<P, N, I> Pool<SimpleTickDataProvider<P, N, I>>
where
    P: Provider<N>,
    N: Network,
    I: TickIndex,
{
    /// Get a V4 [`Pool`] struct with tick data provider from pool parameters
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
    ///
    /// ## Returns
    ///
    /// A [`Pool`] struct with tick data provider
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub async fn from_pool_key_with_tick_data_provider(
        chain_id: ChainId,
        manager: Address,
        currency_a: Address,
        currency_b: Address,
        fee: U24,
        tick_spacing: I,
        hooks: Address,
        provider: P,
        block_id: Option<BlockId>,
    ) -> Result<Self, Error> {
        let pool = Pool::from_pool_key(
            chain_id,
            manager,
            currency_a,
            currency_b,
            fee,
            tick_spacing,
            hooks,
            &provider,
            block_id,
        )
        .await?;
        Self::new_with_tick_data_provider(
            pool.currency0,
            pool.currency1,
            pool.fee,
            tick_spacing,
            pool.hooks,
            pool.sqrt_price_x96,
            pool.liquidity,
            SimpleTickDataProvider::new(manager, pool.pool_id, provider, block_id),
        )
    }
}

/// Creates a Currency from an address and optional metadata
fn create_currency(
    chain_id: ChainId,
    address: Address,
    metadata: Option<(u8, String, String)>,
) -> Currency {
    if address.is_zero() {
        Currency::NativeCurrency(Ether::on_chain(chain_id))
    } else if let Some((decimals, name, symbol)) = metadata {
        Currency::Token(token!(chain_id, address, decimals, symbol, name))
    } else {
        // Placeholder for pool ID calculation when metadata not yet fetched
        Currency::Token(token!(chain_id, address, 18))
    }
}

/// Fetches ERC20 token metadata (decimals, name, symbol) in parallel
async fn fetch_token_metadata<N, P>(
    address: Address,
    provider: P,
    block_id: BlockId,
) -> Result<(u8, String, String), Error>
where
    N: Network,
    P: Provider<N>,
{
    let contract = IERC20Metadata::new(address, provider);
    let decimals = contract.decimals().block(block_id);
    let name = contract.name().block(block_id);
    let symbol = contract.symbol().block(block_id);

    let (decimals, name, symbol) = tokio::join!(decimals.call(), name.call(), symbol.call());

    Ok((decimals?, name?, symbol?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use alloy::providers::{layers::CallBatchLayer, ProviderBuilder};
    use uniswap_v3_sdk::{constants::FeeAmount, entities::TickDataProvider};

    const FEE: FeeAmount = FeeAmount::LOW;
    const TICK_SPACING: i32 = 10;

    #[tokio::test]
    async fn test_from_pool_key() {
        // Use CallBatchLayer to demonstrate recommended setup
        // Note: batching only works for latest block queries
        let provider = ProviderBuilder::new()
            .layer(CallBatchLayer::new())
            .connect_http(RPC_URL.clone());

        // Use the ETH-USDC pool with fee 500 (0.05%) and tick spacing 10
        let pool = Pool::from_pool_key(
            1,
            *POOL_MANAGER_ADDRESS,
            Address::ZERO, // ETH
            USDC.address,
            FEE.into(),
            TICK_SPACING,
            Address::ZERO, // No hooks
            provider,
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
        assert_eq!(
            pool.fee,
            <U24 as From<FeeAmount>>::from(FEE),
            "fee should be 500"
        );
        assert_eq!(pool.tick_spacing, TICK_SPACING, "tick_spacing should be 10");
        assert_eq!(pool.hooks, Address::ZERO, "hooks should be zero address");
    }

    #[tokio::test]
    async fn test_from_pool_key_with_tick_data_provider() {
        let provider = ProviderBuilder::new()
            .layer(CallBatchLayer::new())
            .connect_http(RPC_URL.clone());

        let pool = Pool::from_pool_key_with_tick_data_provider(
            1,
            *POOL_MANAGER_ADDRESS,
            Address::ZERO, // ETH
            USDC.address,
            FEE.into(),
            TICK_SPACING,
            Address::ZERO, // No hooks
            provider,
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

        // Verify tick data provider is working by fetching a tick
        let tick = pool
            .tick_data_provider
            .get_tick(pool.tick_current)
            .await
            .unwrap();
        assert_eq!(tick.index, pool.tick_current);
    }
}
