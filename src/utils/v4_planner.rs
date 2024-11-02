#![allow(unused_imports)]
use crate::prelude::{Error, Pool, Route};
use alloy_dyn_abi::{DynSolType, DynSolValue};
use alloy_primitives::Bytes;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Actions {
    // Pool actions
    // Liquidity actions
    INCREASE_LIQUIDITY = 0x00,
    DECREASE_LIQUIDITY = 0x01,
    MINT_POSITION = 0x02,
    BURN_POSITION = 0x03,
    // Swapping
    SWAP_EXACT_IN_SINGLE = 0x04,
    SWAP_EXACT_IN = 0x05,
    SWAP_EXACT_OUT_SINGLE = 0x06,
    SWAP_EXACT_OUT = 0x07,

    // Closing deltas on the pool manager
    // Settling
    SETTLE = 0x09,
    SETTLE_ALL = 0x10,
    SETTLE_PAIR = 0x11,
    // Taking
    TAKE = 0x12,
    TAKE_ALL = 0x13,
    TAKE_PORTION = 0x14,
    TAKE_PAIR = 0x15,

    SETTLE_TAKE_PAIR = 0x16,

    CLOSE_CURRENCY = 0x17,
    SWEEP = 0x19,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Subparser {
    V4SwapExactInSingle,
    V4SwapExactIn,
    V4SwapExactOutSingle,
    V4SwapExactOut,
    PoolKey,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamType {
    pub name: &'static str,
    pub param_type: &'static str,
    pub subparser: Option<Subparser>,
}

// pub const POOL_KEY_STRUCT: &str = "(address currency0,address currency1,uint24 fee,int24
// tickSpacing,address hooks)";
// pub const PATH_KEY_STRUCT: &str = "(address intermediateCurrency,uint256 fee,int24
// tickSpacing,address hooks,bytes hookData)";
//
// pub const SWAP_EXACT_IN_SINGLE_STRUCT: &str = [
//     "(",
//     POOL_KEY_STRUCT,
//     " poolKey,bool zeroForOne,uint128 amountIn,uint128 amountOutMinimum,uint160
// sqrtPriceLimitX96,bytes hookData)" ].concat()
//
// pub const SWAP_EXACT_IN_STRUCT: &str = [
//     "(address currencyIn,",
//     PATH_KEY_STRUCT,
//     "[] path,uint128 amountIn,uint128 amountOutMinimum)"
// ].concat()
//
// pub const SWAP_EXACT_OUT_SINGLE_STRUCT: &str = [
//     "(",
//     POOL_KEY_STRUCT,
//     " poolKey,bool zeroForOne,uint128 amountOut,uint128 amountInMaximum,uint160
// sqrtPriceLimitX96,bytes hookData)" ].concat()
//
// pub const SWAP_EXACT_OUT_STRUCT: &str = [
//     "(address currencyOut,",
//     PATH_KEY_STRUCT,
//     "[] path,uint128 amountOut,uint128 amountInMaximum)"
// ].concat()

fn currency_address(currency: &impl BaseCurrency) -> Address {
    if currency.is_native() {
        Address::ZERO
    } else {
        currency.wrapped().address()
    }
}

struct RouterAction {
    action: Actions,
    encoded_input: Bytes,
}

fn create_action(action: Actions, parameters: Vec<ParamType>) -> Result<RouterAction, Error> {
    unimplemented!("create_action")
}
