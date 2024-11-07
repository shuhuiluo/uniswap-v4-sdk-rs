use super::v4_planner::{Actions, V4Planner};
use crate::{
    abi::{
        BurnPositionParams, DecreaseLiquidityParams, IncreaseLiquidityParams, MintPositionParams,
        SettlePairParams, SweepParams, TakePairParams,
    },
    entities::Pool,
};
use alloy_primitives::{aliases::I24, Address, Bytes, U256};
use uniswap_sdk_core::prelude::{BaseCurrency, Currency};

/// Planner for managing Uniswap V4 liquidity positions
/// Handles operations like minting, burning, and modifying positions

#[derive(Clone, Debug, Default, PartialEq)]
pub struct V4PositionPlanner {
    /// Creates a new V4PositionPlanner instance
    pub planner: V4Planner,
}

impl V4PositionPlanner {
    pub fn new() -> Self {
        Self {
            planner: V4Planner::default(),
        }
    }

    /// Adds a mint position action to the planner
    ///
    /// # Arguments
    /// * `pool` - Reference to the `[Pool]`
    /// * `tick_lower` - Lower tick boundary of the position
    /// * `tick_upper` - Upper tick boundary of the position
    /// * `liquidity` - Amount of liquidity to mint
    /// * `amount0_max` - Maximum amount of token0 to use
    /// * `amount1_max` - Maximum amount of token1 to use
    /// * `owner` - Address that will own the minted position
    /// * `hook_data` - Additional data to be passed to hooks
    pub fn add_mint(
        &mut self,
        pool: &Pool,
        tick_lower: i32,
        tick_upper: i32,
        liquidity: U256,
        amount0_max: u128,
        amount1_max: u128,
        owner: Address,
        hook_data: Bytes,
    ) {
        let ticks = I24::unchecked_from(pool.tick_spacing);

        let pool_key = Pool::get_pool_key(
            &pool.currency0,
            &pool.currency1,
            pool.fee,
            ticks,
            pool.hooks,
        )
        .unwrap_or_default();

        self.planner
            .add_action(&Actions::MINT_POSITION(MintPositionParams {
                poolKey: pool_key,
                tickLower: I24::unchecked_from(tick_lower),
                tickUpper: I24::unchecked_from(tick_upper),
                liquidity,
                amount0Max: amount0_max,
                amount1Max: amount1_max,
                owner,
                hookData: hook_data,
            }));
    }

    /// Adds an increase liquidity action to the planner
    /// 
    /// # Arguments
    /// * `token_id` - ID of the position to increase liquidity for
    /// * `liquidity` - Amount of liquidity to add
    /// * `amount0_max` - Maximum amount of token0 to use
    /// * `amount1_max` - Maximum amount of token1 to use
    /// * `hook_data` - Additional data to be passed to hooks
    pub fn add_increase(
        &mut self,
        token_id: U256,
        liquidity: U256,
        amount0_max: u128,
        amount1_max: u128,
        hook_data: Bytes,
    ) {
        self.planner
            .add_action(&Actions::INCREASE_LIQUIDITY(IncreaseLiquidityParams {
                tokenId: token_id,
                liquidity,
                amount0Max: amount0_max,
                amount1Max: amount1_max,
                hookData: hook_data,
            }));
    }

    /// Adds a decrease liquidity action to the planner
    /// 
    /// # Arguments
    /// * `token_id` - ID of the position to decrease liquidity for
    /// * `liquidity` - Amount of liquidity to remove
    /// * `amount0_min` - Minimum amount of token0 to receive
    /// * `amount1_min` - Minimum amount of token1 to receive
    /// * `hook_data` - Additional data to be passed to hooks
    pub fn add_decrease(
        &mut self,
        token_id: U256,
        liquidity: U256,
        amount0_min: u128,
        amount1_min: u128,
        hook_data: Bytes,
    ) {
        self.planner
            .add_action(&Actions::DECREASE_LIQUIDITY(DecreaseLiquidityParams {
                tokenId: token_id,
                liquidity,
                amount0Min: amount0_min,
                amount1Min: amount1_min,
                hookData: hook_data,
            }));
    }

    
    /// Adds a burn position action to the planner
    /// 
    /// # Arguments
    /// * `token_id` - ID of the position to burn
    /// * `amount0_min` - Minimum amount of token0 to receive
    /// * `amount1_min` - Minimum amount of token1 to receive
    /// * `hook_data` - Additional data to be passed to hooks
    pub fn add_burn(
        &mut self,
        token_id: U256,
        amount0_min: u128,
        amount1_min: u128,
        hook_data: Bytes,
    ) {
        self.planner
            .add_action(&Actions::BURN_POSITION(BurnPositionParams {
                tokenId: token_id,
                amount0Min: amount0_min,
                amount1Min: amount1_min,
                hookData: hook_data,
            }));
    }

    /// Adds a settle pair action to the planner
    /// 
    /// # Arguments
    /// * `currency0` - First token in the pair
    /// * `currency1` - Second token in the pair
    pub fn add_settle_pair(&mut self, currency0: &Currency, currency1: &Currency) {
        self.planner
            .add_action(&Actions::SETTLE_PAIR(SettlePairParams {
                currency0: currency0.address(),
                currency1: currency1.address(),
            }));
    }

    
    /// Adds a take pair action to the planner
    /// 
    /// # Arguments
    /// * `currency0` - First token in the pair
    /// * `currency1` - Second token in the pair
    /// * `recipient` - Address to receive the tokens
    pub fn add_take_pair(
        &mut self,
        currency0: &Currency,
        currency1: &Currency,
        recipient: Address,
    ) {
        self.planner.add_action(&Actions::TAKE_PAIR(TakePairParams {
            currency0: currency0.address(),
            currency1: currency1.address(),
            recipient,
        }));
    }

    /// Adds a sweep action to the planner
    /// 
    /// # Arguments
    /// * `currency` - Token to sweep
    /// * `recipient` - Address to receive the tokens
    pub fn add_sweep(&mut self, currency: &Currency, recipient: Address) {
        self.planner.add_action(&Actions::SWEEP(SweepParams {
            currency: currency.address(),
            recipient,
        }));
    }
}
