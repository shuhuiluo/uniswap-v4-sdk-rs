use crate::abi::IExtsload;
use alloy::{
    eips::{BlockId, BlockNumberOrTag},
    network::{Ethereum, Network},
    providers::Provider,
    uint,
};
use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_sol_types::SolValue;
use uniswap_v3_sdk::prelude::*;

const POOLS_SLOT: U256 = uint!(6_U256);
const TICKS_OFFSET: U256 = uint!(4_U256);
const TICK_BITMAP_OFFSET: U256 = uint!(5_U256);

fn get_pool_state_slot(pool_id: B256) -> U256 {
    U256::from_be_bytes(keccak256((pool_id, POOLS_SLOT).abi_encode()).0)
}

fn get_tick_bitmap_slot<I: TickIndex>(pool_id: B256, tick: I) -> U256 {
    let state_slot = get_pool_state_slot(pool_id);
    let tick_bitmap_mapping = state_slot + TICK_BITMAP_OFFSET;
    U256::from_be_bytes(keccak256((tick.to_i24().as_i16(), tick_bitmap_mapping).abi_encode()).0)
}

fn get_tick_info_slot<I: TickIndex>(pool_id: B256, tick: I) -> U256 {
    let state_slot = get_pool_state_slot(pool_id);
    let ticks_mapping_slot = state_slot + TICKS_OFFSET;
    U256::from_be_bytes(keccak256((tick.to_i24(), ticks_mapping_slot).abi_encode()).0)
}

/// A lens for querying Uniswap V4 pool manager
#[derive(Clone, Debug)]
pub struct PoolManagerLens<P, N = Ethereum>
where
    N: Network,
    P: Provider<N>,
{
    pub manager: IExtsload::IExtsloadInstance<(), P, N>,
    _network: core::marker::PhantomData<N>,
}

impl<P, N> PoolManagerLens<P, N>
where
    N: Network,
    P: Provider<N>,
{
    /// Creates a new `PoolManagerLens`
    #[inline]
    pub const fn new(manager: Address, provider: P) -> Self {
        Self {
            manager: IExtsload::new(manager, provider),
            _network: core::marker::PhantomData,
        }
    }

    /// Retrieves the tick bitmap of a pool at a specific tick
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `tick`: The tick to retrieve the bitmap for
    /// * `block_id`: Optional block ID to query at
    #[inline]
    pub async fn get_tick_bitmap<I: TickIndex>(
        &self,
        pool_id: B256,
        tick: I,
        block_id: Option<BlockId>,
    ) -> Result<U256, Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let slot = get_tick_bitmap_slot(pool_id, tick);
        let word = self
            .manager
            .extsload_0(B256::from(slot))
            .block(block_id)
            .call()
            .await?;
        Ok(U256::from_be_bytes(word.value.0))
    }

    /// Retrieves the liquidity information of a pool at a specific tick
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `tick`: The tick to retrieve liquidity for
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `liquidity_gross`: The total position liquidity that references this tick
    /// * `liquidity_net`: The amount of net liquidity added (subtracted) when tick is crossed from
    ///   left to right (right to left)
    #[inline]
    pub async fn get_tick_liquidity<I: TickIndex>(
        &self,
        pool_id: B256,
        tick: I,
        block_id: Option<BlockId>,
    ) -> Result<(u128, i128), Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let slot = get_tick_info_slot(pool_id, tick);
        let value = self
            .manager
            .extsload_0(B256::from(slot))
            .block(block_id)
            .call()
            .await?
            .value;
        // In Solidity:
        // liquidityNet := sar(128, value)
        // liquidityGross := and(value, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF)
        let liquidity_gross = u128::from_be_bytes(value.0[16..32].try_into().unwrap());
        let liquidity_net = i128::from_be_bytes(value.0[0..16].try_into().unwrap());
        Ok((liquidity_gross, liquidity_net))
    }
}
