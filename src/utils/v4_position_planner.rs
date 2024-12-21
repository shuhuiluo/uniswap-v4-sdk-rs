use crate::prelude::*;
use alloy_primitives::{Address, Bytes, U256};
use derive_more::{Deref, DerefMut};
use uniswap_sdk_core::prelude::BaseCurrency;
use uniswap_v3_sdk::prelude::{TickDataProvider, TickIndex};

#[derive(Clone, Debug, Default, PartialEq, Deref, DerefMut)]
pub struct V4PositionPlanner(pub V4Planner);

impl V4PositionPlanner {
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn add_mint<TP: TickDataProvider>(
        &mut self,
        pool: &Pool<TP>,
        tick_lower: TP::Index,
        tick_upper: TP::Index,
        liquidity: U256,
        amount0_max: u128,
        amount1_max: u128,
        owner: Address,
        hook_data: Bytes,
    ) {
        self.add_action(&Actions::MINT_POSITION(MintPositionParams {
            poolKey: pool.pool_key.clone(),
            tickLower: tick_lower.to_i24(),
            tickUpper: tick_upper.to_i24(),
            liquidity,
            amount0Max: amount0_max,
            amount1Max: amount1_max,
            owner,
            hookData: hook_data,
        }));
    }

    #[inline]
    pub fn add_increase(
        &mut self,
        token_id: U256,
        liquidity: U256,
        amount0_max: u128,
        amount1_max: u128,
        hook_data: Bytes,
    ) {
        self.add_action(&Actions::INCREASE_LIQUIDITY(IncreaseLiquidityParams {
            tokenId: token_id,
            liquidity,
            amount0Max: amount0_max,
            amount1Max: amount1_max,
            hookData: hook_data,
        }));
    }

    #[inline]
    pub fn add_decrease(
        &mut self,
        token_id: U256,
        liquidity: U256,
        amount0_min: u128,
        amount1_min: u128,
        hook_data: Bytes,
    ) {
        self.add_action(&Actions::DECREASE_LIQUIDITY(DecreaseLiquidityParams {
            tokenId: token_id,
            liquidity,
            amount0Min: amount0_min,
            amount1Min: amount1_min,
            hookData: hook_data,
        }));
    }

    #[inline]
    pub fn add_burn(
        &mut self,
        token_id: U256,
        amount0_min: u128,
        amount1_min: u128,
        hook_data: Bytes,
    ) {
        self.add_action(&Actions::BURN_POSITION(BurnPositionParams {
            tokenId: token_id,
            amount0Min: amount0_min,
            amount1Min: amount1_min,
            hookData: hook_data,
        }));
    }

    #[inline]
    pub fn add_settle_pair(
        &mut self,
        currency0: &impl BaseCurrency,
        currency1: &impl BaseCurrency,
    ) {
        self.add_action(&Actions::SETTLE_PAIR(SettlePairParams {
            currency0: to_address(currency0),
            currency1: to_address(currency1),
        }));
    }

    #[inline]
    pub fn add_take_pair(
        &mut self,
        currency0: &impl BaseCurrency,
        currency1: &impl BaseCurrency,
        recipient: Address,
    ) {
        self.add_action(&Actions::TAKE_PAIR(TakePairParams {
            currency0: to_address(currency0),
            currency1: to_address(currency1),
            recipient,
        }));
    }

    #[inline]
    pub fn add_sweep(&mut self, currency: &impl BaseCurrency, recipient: Address) {
        self.add_action(&Actions::SWEEP(SweepParams {
            currency: to_address(currency),
            recipient,
        }));
    }
}
