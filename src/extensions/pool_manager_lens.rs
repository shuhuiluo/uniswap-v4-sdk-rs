//! ## Pool Manager Lens
//! This module provides a lens for querying the Uniswap V4 pool manager. It is similar to
//! [`StateView`](https://github.com/Uniswap/v4-periphery/blob/main/src/lens/StateView.sol), but
//! does the slot calculation and ABI decoding in Rust instead of Solidity. It does not require
//! contract deployment and uses `extsload` to read the state under the hood.

use crate::prelude::{Error, IExtsload};
use alloy::{
    eips::BlockId,
    network::{Ethereum, Network},
    providers::Provider,
    uint,
};
use alloy_primitives::{
    aliases::{I24, U24},
    keccak256, Address, B256, U160, U256,
};
use alloy_sol_types::SolValue;
use uniswap_v3_sdk::prelude::*;

const POOLS_SLOT: U256 = uint!(6_U256);
const FEE_GROWTH_GLOBAL0_OFFSET: U256 = uint!(1_U256);
const LIQUIDITY_OFFSET: U256 = uint!(3_U256);
const TICKS_OFFSET: U256 = uint!(4_U256);
const TICK_BITMAP_OFFSET: U256 = uint!(5_U256);
const POSITIONS_OFFSET: U256 = uint!(6_U256);

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

fn get_position_info_slot(pool_id: B256, position_key: B256) -> U256 {
    let state_slot = get_pool_state_slot(pool_id);
    let position_mapping_slot = state_slot + POSITIONS_OFFSET;
    U256::from_be_bytes(keccak256((position_key, position_mapping_slot).abi_encode()).0)
}

/// A lens for querying Uniswap V4 pool manager
#[derive(Clone, Debug)]
pub struct PoolManagerLens<P, N = Ethereum>
where
    N: Network,
    P: Provider<N>,
{
    pub manager: IExtsload::IExtsloadInstance<P, N>,
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

    /// Retrieves the Slot0 of a pool: sqrtPriceX96, tick, protocolFee, lpFee
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `sqrtPriceX96`: The square root of the price of the pool, in Q96 precision
    /// * `tick`: The current tick of the pool
    /// * `protocol_fee`: The protocol fee of the pool
    /// * `lp_fee`: The swap fee of the pool
    #[inline]
    pub async fn get_slot0(
        &self,
        pool_id: B256,
        block_id: Option<BlockId>,
    ) -> Result<(U160, I24, U24, U24), Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let state_slot = get_pool_state_slot(pool_id);
        let data = self
            .manager
            .extsload_0(B256::from(state_slot))
            .block(block_id)
            .call()
            .await?;

        let sqrt_price_x96 = U160::from_be_slice(&data[12..32]);

        let tick_bytes = unsafe { (data.as_ptr().add(9) as *const [u8; 3]).read_unaligned() };
        let tick = I24::from_be_bytes(tick_bytes);

        let protocol_fee_bytes =
            unsafe { (data.as_ptr().add(6) as *const [u8; 3]).read_unaligned() };
        let protocol_fee = U24::from_be_bytes(protocol_fee_bytes);

        let lp_fee_bytes = unsafe { (data.as_ptr().add(3) as *const [u8; 3]).read_unaligned() };
        let lp_fee = U24::from_be_bytes(lp_fee_bytes);

        Ok((sqrt_price_x96, tick, protocol_fee, lp_fee))
    }

    /// Retrieves full tick information from a pool at a specific tick
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `tick`: The tick to retrieve information for
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `liquidity_gross`: The total position liquidity that references this tick
    /// * `liquidity_net`: The amount of net liquidity added (subtracted) when tick is crossed from
    ///   left to right (right to left)
    /// * `fee_growth_outside0_x128`: Fee growth per unit of liquidity on the other side of this
    ///   tick for token0
    /// * `fee_growth_outside1_x128`: Fee growth per unit of liquidity on the other side of this
    ///   tick for token1
    #[inline]
    pub async fn get_tick_info<I: TickIndex>(
        &self,
        pool_id: B256,
        tick: I,
        block_id: Option<BlockId>,
    ) -> Result<(u128, i128, U256, U256), Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = get_tick_info_slot(pool_id, tick);
        let data = self
            .manager
            .extsload_1(B256::from(slot), uint!(3_U256))
            .block(block_id)
            .call()
            .await?;

        let (liquidity_gross, liquidity_net) = decode_liquidity_gross_and_net(data[0]);
        let fee_growth_outside0_x128 = U256::from_be_bytes(data[1].0);
        let fee_growth_outside1_x128 = U256::from_be_bytes(data[2].0);

        Ok((
            liquidity_gross,
            liquidity_net,
            fee_growth_outside0_x128,
            fee_growth_outside1_x128,
        ))
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
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = get_tick_info_slot(pool_id, tick);
        let value = self
            .manager
            .extsload_0(B256::from(slot))
            .block(block_id)
            .call()
            .await?;
        Ok(decode_liquidity_gross_and_net(value))
    }

    /// Retrieves the fee growth outside a tick of a pool
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `tick`: The tick to retrieve fee growth for
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `fee_growth_outside0_x128`: Fee growth per unit of liquidity on the other side of this
    ///   tick for token0
    /// * `fee_growth_outside1_x128`: Fee growth per unit of liquidity on the other side of this
    ///   tick for token1
    #[inline]
    pub async fn get_tick_fee_growth_outside<I: TickIndex>(
        &self,
        pool_id: B256,
        tick: I,
        block_id: Option<BlockId>,
    ) -> Result<(U256, U256), Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = B256::from(get_tick_info_slot(pool_id, tick) + uint!(1_U256));
        let data = self
            .manager
            .extsload_1(slot, uint!(2_U256))
            .block(block_id)
            .call()
            .await?;

        let fee_growth_outside0_x128 = U256::from_be_bytes(data[0].0);
        let fee_growth_outside1_x128 = U256::from_be_bytes(data[1].0);

        Ok((fee_growth_outside0_x128, fee_growth_outside1_x128))
    }

    /// Retrieves the global fee growth of a pool
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `fee_growth_global0`: The global fee growth for token0
    /// * `fee_growth_global1`: The global fee growth for token1
    #[inline]
    pub async fn get_fee_growth_globals(
        &self,
        pool_id: B256,
        block_id: Option<BlockId>,
    ) -> Result<(U256, U256), Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let state_slot = get_pool_state_slot(pool_id);
        let slot_fee_growth_global0 = B256::from(state_slot + FEE_GROWTH_GLOBAL0_OFFSET);
        let data = self
            .manager
            .extsload_1(slot_fee_growth_global0, uint!(2_U256))
            .block(block_id)
            .call()
            .await?;

        let fee_growth_global0 = U256::from_be_bytes(data[0].0);
        let fee_growth_global1 = U256::from_be_bytes(data[1].0);

        Ok((fee_growth_global0, fee_growth_global1))
    }

    /// Retrieves the total liquidity of a pool
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `liquidity`: The liquidity of the pool
    #[inline]
    pub async fn get_liquidity(
        &self,
        pool_id: B256,
        block_id: Option<BlockId>,
    ) -> Result<u128, Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = B256::from(get_pool_state_slot(pool_id) + LIQUIDITY_OFFSET);
        let value = self.manager.extsload_0(slot).block(block_id).call().await?;
        Ok(decode_liquidity(value))
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
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = get_tick_bitmap_slot(pool_id, tick);
        let word = self
            .manager
            .extsload_0(B256::from(slot))
            .block(block_id)
            .call()
            .await?;
        Ok(U256::from_be_bytes(word.0))
    }

    /// Retrieves the position information of a pool at a specific position key
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `position_key`: The key of the position
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `liquidity`: The liquidity of the position
    /// * `fee_growth_inside0_last_x128`: The fee growth inside the position for token0
    /// * `fee_growth_inside1_last_x128`: The fee growth inside the position for token1
    #[inline]
    pub async fn get_position_info(
        &self,
        pool_id: B256,
        position_key: B256,
        block_id: Option<BlockId>,
    ) -> Result<(u128, U256, U256), Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = get_position_info_slot(pool_id, position_key);
        let data = self
            .manager
            .extsload_1(B256::from(slot), uint!(3_U256))
            .block(block_id)
            .call()
            .await?;

        let liquidity = decode_liquidity(data[0]);
        let fee_growth_inside0_last_x128 = U256::from_be_bytes(data[1].0);
        let fee_growth_inside1_last_x128 = U256::from_be_bytes(data[2].0);

        Ok((
            liquidity,
            fee_growth_inside0_last_x128,
            fee_growth_inside1_last_x128,
        ))
    }

    /// Retrieves just the liquidity of a position
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `position_key`: The key of the position
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `liquidity`: The liquidity of the position
    #[inline]
    pub async fn get_position_liquidity(
        &self,
        pool_id: B256,
        position_key: B256,
        block_id: Option<BlockId>,
    ) -> Result<u128, Error> {
        let block_id = block_id.unwrap_or(BlockId::latest());
        let slot = get_position_info_slot(pool_id, position_key);
        let value = self
            .manager
            .extsload_0(B256::from(slot))
            .block(block_id)
            .call()
            .await?;
        Ok(decode_liquidity(value))
    }

    /// Calculates the fee growth inside a tick range of a pool
    ///
    /// ## Arguments
    ///
    /// * `pool_id`: The ID of the pool
    /// * `tick_lower`: The lower tick of the range
    /// * `tick_upper`: The upper tick of the range
    /// * `block_id`: Optional block ID to query at
    ///
    /// ## Returns
    ///
    /// * `fee_growth_inside0_x128`: The fee growth inside the tick range for token0
    /// * `fee_growth_inside1_x128`: The fee growth inside the tick range for token1
    #[inline]
    pub async fn get_fee_growth_inside<I: TickIndex>(
        &self,
        pool_id: B256,
        tick_lower: I,
        tick_upper: I,
        block_id: Option<BlockId>,
    ) -> Result<(U256, U256), Error> {
        let (fee_growth_global0_x128, fee_growth_global1_x128) =
            self.get_fee_growth_globals(pool_id, block_id).await?;

        let (lower_fee_growth_outside0_x128, lower_fee_growth_outside1_x128) = self
            .get_tick_fee_growth_outside(pool_id, tick_lower, block_id)
            .await?;

        let (upper_fee_growth_outside0_x128, upper_fee_growth_outside1_x128) = self
            .get_tick_fee_growth_outside(pool_id, tick_upper, block_id)
            .await?;

        let (_, tick_current, _, _) = self.get_slot0(pool_id, block_id).await?;

        let (fee_growth_inside0_x128, fee_growth_inside1_x128) =
            if tick_current < tick_lower.to_i24() {
                (
                    lower_fee_growth_outside0_x128 - upper_fee_growth_outside0_x128,
                    lower_fee_growth_outside1_x128 - upper_fee_growth_outside1_x128,
                )
            } else if tick_current >= tick_upper.to_i24() {
                (
                    upper_fee_growth_outside0_x128 - lower_fee_growth_outside0_x128,
                    upper_fee_growth_outside1_x128 - lower_fee_growth_outside1_x128,
                )
            } else {
                (
                    fee_growth_global0_x128
                        - lower_fee_growth_outside0_x128
                        - upper_fee_growth_outside0_x128,
                    fee_growth_global1_x128
                        - lower_fee_growth_outside1_x128
                        - upper_fee_growth_outside1_x128,
                )
            };

        Ok((fee_growth_inside0_x128, fee_growth_inside1_x128))
    }
}

const fn decode_liquidity_gross_and_net(word: B256) -> (u128, i128) {
    // In Solidity:
    // liquidityNet := sar(128, value)
    // liquidityGross := and(value, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF)
    let liquidity_gross = decode_liquidity(word);
    let liquidity_net = unsafe {
        // Create a pointer to the start of the first half of the array
        let net_ptr = word.0.as_ptr() as *const i128;
        // Read the value in big-endian format
        i128::from_be(net_ptr.read_unaligned())
    };
    (liquidity_gross, liquidity_net)
}

const fn decode_liquidity(word: B256) -> u128 {
    unsafe {
        // Create a pointer to the start of the second half of the array
        let ptr = word.0.as_ptr().add(16) as *const u128;
        // Read the value in big-endian format
        u128::from_be(ptr.read_unaligned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prelude::*, tests::*};
    use alloy::providers::RootProvider;
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::addresses::CHAIN_TO_ADDRESSES_MAP;

    const TICK_SPACING: i32 = 10;
    static POOL_MANAGER: Lazy<PoolManagerLens<RootProvider>> = Lazy::new(|| {
        PoolManagerLens::new(
            CHAIN_TO_ADDRESSES_MAP
                .get(&1)
                .unwrap()
                .v4_pool_manager
                .unwrap(),
            PROVIDER.clone(),
        )
    });

    #[tokio::test]
    async fn test_get_slot0() {
        let (sqrt_price_x96_lens, tick_lens, protocol_fee_lens, lp_fee_lens) = POOL_MANAGER
            .get_slot0(*POOL_ID_ETH_USDC, BLOCK_ID)
            .await
            .unwrap();

        let slot0_state_view = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        assert_ne!(sqrt_price_x96_lens, U160::ZERO);
        assert_eq!(
            sqrt_price_x96_lens, slot0_state_view.sqrtPriceX96,
            "sqrtPriceX96 mismatch"
        );
        assert_eq!(tick_lens, slot0_state_view.tick, "tick mismatch");
        assert_eq!(
            protocol_fee_lens, slot0_state_view.protocolFee,
            "protocolFee mismatch"
        );
        assert_eq!(lp_fee_lens, slot0_state_view.lpFee, "lpFee mismatch");
    }

    macro_rules! assert_tick_info_match {
        ($pool_id:expr, $tick:expr, $block_id:expr) => {
            let (
                liquidity_gross_lens,
                liquidity_net_lens,
                fee_growth_outside0_x128_lens,
                fee_growth_outside1_x128_lens,
            ) = POOL_MANAGER
                .get_tick_info($pool_id, $tick, $block_id)
                .await
                .unwrap();
            let tick_info = STATE_VIEW
                .getTickInfo($pool_id, $tick.to_i24())
                .block($block_id.unwrap())
                .call()
                .await
                .unwrap();

            assert_ne!(liquidity_gross_lens, 0);
            assert_eq!(
                liquidity_gross_lens, tick_info.liquidityGross,
                "liquidityGross"
            );
            assert_ne!(liquidity_net_lens, 0);
            assert_eq!(liquidity_net_lens, tick_info.liquidityNet, "liquidityNet");
            assert_eq!(
                fee_growth_outside0_x128_lens, tick_info.feeGrowthOutside0X128,
                "feeGrowthOutside0X128"
            );
            assert_eq!(
                fee_growth_outside1_x128_lens, tick_info.feeGrowthOutside1X128,
                "feeGrowthOutside1X128"
            );
        };
    }

    async fn nearest_populated_tick(tick: I24) -> i32 {
        let word = tick.as_i32().compress(TICK_SPACING).position().0;
        let bitmap = POOL_MANAGER
            .get_tick_bitmap(*POOL_ID_ETH_USDC, word, BLOCK_ID)
            .await
            .unwrap();
        let msb = most_significant_bit(bitmap);
        ((word << 8) + msb as i32) * TICK_SPACING
    }

    #[tokio::test]
    async fn test_get_tick_info() {
        let slot0 = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        let tick = nearest_populated_tick(slot0.tick).await;
        assert_tick_info_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MIN_TICK_I32, TICK_SPACING);
        assert_tick_info_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MAX_TICK_I32, TICK_SPACING);
        assert_tick_info_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);
    }

    macro_rules! assert_tick_liquidity_match {
        ($pool_id:expr, $tick:expr, $block_id:expr) => {
            let (liquidity_gross_lens, liquidity_net_lens) = POOL_MANAGER
                .get_tick_liquidity($pool_id, $tick, $block_id)
                .await
                .unwrap();
            let tick_liquidity = STATE_VIEW
                .getTickLiquidity($pool_id, $tick.to_i24())
                .block($block_id.unwrap())
                .call()
                .await
                .unwrap();

            assert_ne!(liquidity_gross_lens, 0);
            assert_eq!(
                liquidity_gross_lens, tick_liquidity.liquidityGross,
                "liquidityGross"
            );
            assert_ne!(liquidity_net_lens, 0);
            assert_eq!(
                liquidity_net_lens, tick_liquidity.liquidityNet,
                "liquidityNet"
            );
        };
    }

    #[tokio::test]
    async fn test_get_tick_liquidity() {
        let slot0 = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        let tick = nearest_populated_tick(slot0.tick).await;
        assert_tick_liquidity_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MIN_TICK_I32, TICK_SPACING);
        assert_tick_liquidity_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MAX_TICK_I32, TICK_SPACING);
        assert_tick_liquidity_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);
    }

    macro_rules! assert_fee_growth_outside_match {
        ($pool_id:expr, $tick:expr, $block_id:expr) => {
            let (fee_growth_outside0_x128_lens, fee_growth_outside1_x128_lens) = POOL_MANAGER
                .get_tick_fee_growth_outside($pool_id, $tick, $block_id)
                .await
                .unwrap();
            let fee_growth_outside = STATE_VIEW
                .getTickFeeGrowthOutside($pool_id, $tick.to_i24())
                .block($block_id.unwrap())
                .call()
                .await
                .unwrap();

            assert_eq!(
                fee_growth_outside0_x128_lens, fee_growth_outside.feeGrowthOutside0X128,
                "feeGrowthOutside0X128"
            );
            assert_eq!(
                fee_growth_outside1_x128_lens, fee_growth_outside.feeGrowthOutside1X128,
                "feeGrowthOutside1X128"
            );
        };
    }

    #[tokio::test]
    async fn test_get_tick_fee_growth_outside() {
        let slot0 = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        let tick = nearest_populated_tick(slot0.tick).await;
        assert_fee_growth_outside_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MIN_TICK_I32, TICK_SPACING);
        assert_fee_growth_outside_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);

        let tick = nearest_usable_tick(MAX_TICK_I32, TICK_SPACING);
        assert_fee_growth_outside_match!(*POOL_ID_ETH_USDC, tick, BLOCK_ID);
    }

    #[tokio::test]
    async fn test_get_fee_growth_globals() {
        let (fee_growth_global0_lens, fee_growth_global1_lens) = POOL_MANAGER
            .get_fee_growth_globals(*POOL_ID_ETH_USDC, BLOCK_ID)
            .await
            .unwrap();
        let fee_growth_globals = STATE_VIEW
            .getFeeGrowthGlobals(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        assert_eq!(
            fee_growth_global0_lens, fee_growth_globals.feeGrowthGlobal0,
            "feeGrowthGlobal0"
        );
        assert_eq!(
            fee_growth_global1_lens, fee_growth_globals.feeGrowthGlobal1,
            "feeGrowthGlobal1"
        );
    }

    #[tokio::test]
    async fn test_get_liquidity() {
        let liquidity_lens = POOL_MANAGER
            .get_liquidity(*POOL_ID_ETH_USDC, BLOCK_ID)
            .await
            .unwrap();
        let liquidity = STATE_VIEW
            .getLiquidity(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        assert_eq!(liquidity_lens, liquidity);
    }

    macro_rules! assert_tick_bitmap_match {
        ($pool_id:expr, $pos:expr, $block_id:expr) => {
            let bitmap_lens = POOL_MANAGER
                .get_tick_bitmap($pool_id, $pos, $block_id)
                .await
                .unwrap();
            let bitmap_state_view = STATE_VIEW
                .getTickBitmap($pool_id, $pos as i16)
                .block($block_id.unwrap())
                .call()
                .await
                .unwrap();

            assert_ne!(bitmap_lens, U256::ZERO);
            assert_eq!(bitmap_lens, bitmap_state_view);
        };
    }

    #[tokio::test]
    async fn test_get_tick_bitmap() {
        let slot0 = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        let word = slot0.tick.as_i32().compress(TICK_SPACING).position().0;
        for pos in word - 2..=word + 2 {
            assert_tick_bitmap_match!(*POOL_ID_ETH_USDC, pos, BLOCK_ID);
        }

        let word = MIN_TICK_I32.compress(TICK_SPACING).position().0;
        assert_tick_bitmap_match!(*POOL_ID_ETH_USDC, word, BLOCK_ID);

        let word = MAX_TICK_I32.compress(TICK_SPACING).position().0;
        assert_tick_bitmap_match!(*POOL_ID_ETH_USDC, word, BLOCK_ID);
    }

    async fn get_position_keys() -> Vec<B256> {
        get_position_keys_in_blocks(
            *POOL_MANAGER.manager.address(),
            *POOL_ID_ETH_USDC,
            (BLOCK_ID.unwrap().as_u64().unwrap() - 499).into(),
            BLOCK_ID.unwrap().as_u64().unwrap().into(),
            &*PROVIDER,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_get_position_info() {
        let position_keys = get_position_keys().await;
        assert!(!position_keys.is_empty());

        for position_key in position_keys {
            let (
                liquidity_lens,
                fee_growth_inside0_last_x128_lens,
                fee_growth_inside1_last_x128_lens,
            ) = POOL_MANAGER
                .get_position_info(*POOL_ID_ETH_USDC, position_key, BLOCK_ID)
                .await
                .unwrap();
            let position_info = STATE_VIEW
                .getPositionInfo_1(*POOL_ID_ETH_USDC, position_key)
                .block(BLOCK_ID.unwrap())
                .call()
                .await
                .unwrap();

            assert_eq!(liquidity_lens, position_info.liquidity);
            assert_eq!(
                fee_growth_inside0_last_x128_lens, position_info.feeGrowthInside0LastX128,
                "feeGrowthInside0LastX128"
            );
            assert_eq!(
                fee_growth_inside1_last_x128_lens, position_info.feeGrowthInside1LastX128,
                "feeGrowthInside1LastX128"
            );
        }
    }

    #[tokio::test]
    async fn test_get_position_liquidity() {
        let position_keys = get_position_keys().await;
        assert!(!position_keys.is_empty());

        for position_key in position_keys {
            let liquidity_lens = POOL_MANAGER
                .get_position_liquidity(*POOL_ID_ETH_USDC, position_key, BLOCK_ID)
                .await
                .unwrap();
            let liquidity = STATE_VIEW
                .getPositionLiquidity(*POOL_ID_ETH_USDC, position_key)
                .block(BLOCK_ID.unwrap())
                .call()
                .await
                .unwrap();

            assert_eq!(liquidity_lens, liquidity);
        }
    }

    #[tokio::test]
    async fn test_get_fee_growth_inside() {
        let slot0 = STATE_VIEW
            .getSlot0(*POOL_ID_ETH_USDC)
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        let tick = nearest_populated_tick(slot0.tick).await;
        let tick_lower = tick - TICK_SPACING;
        let tick_upper = tick + TICK_SPACING;
        let (fee_growth_inside0_lens, fee_growth_inside1_lens) = POOL_MANAGER
            .get_fee_growth_inside(*POOL_ID_ETH_USDC, tick_lower, tick_upper, BLOCK_ID)
            .await
            .unwrap();
        let fee_growth_inside = STATE_VIEW
            .getFeeGrowthInside(*POOL_ID_ETH_USDC, tick_lower.to_i24(), tick_upper.to_i24())
            .block(BLOCK_ID.unwrap())
            .call()
            .await
            .unwrap();

        assert_eq!(
            fee_growth_inside0_lens, fee_growth_inside.feeGrowthInside0X128,
            "feeGrowthInside0X128"
        );
        assert_eq!(
            fee_growth_inside1_lens, fee_growth_inside.feeGrowthInside1X128,
            "feeGrowthInside1X128"
        );
    }
}
