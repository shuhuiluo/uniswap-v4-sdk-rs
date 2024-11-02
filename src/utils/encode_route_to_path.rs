use crate::prelude::{Pool, Route};
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::sol;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

sol! {
    #[derive(Debug, PartialEq)]
    struct PathKey {
        address intermediateCurrency;
        uint256 fee;
        int24 tickSpacing;
        address hooks;
        bytes hookData;
    }
}

#[inline]
pub fn encode_route_to_path<TInput, TOutput, TP>(
    route: &Route<TInput, TOutput, TP>,
    exact_output: bool,
) -> Vec<PathKey>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    let mut path_keys: Vec<PathKey> = Vec::with_capacity(route.pools.len());
    if exact_output {
        let mut output_currency = &route.path_output;
        for pool in route.pools.iter().rev() {
            let (next_currency, key) = get_next_path_key(pool, output_currency);
            path_keys.push(key);
            output_currency = next_currency;
        }
        path_keys.reverse();
    } else {
        let mut input_currency = &route.path_input;
        for pool in &route.pools {
            let (next_currency, key) = get_next_path_key(pool, input_currency);
            path_keys.push(key);
            input_currency = next_currency;
        }
    }
    path_keys
}

#[inline]
fn get_next_path_key<'a, TInput, TP>(
    pool: &'a Pool<TP>,
    input_currency: &'a TInput,
) -> (&'a Currency, PathKey)
where
    TInput: BaseCurrency,
    TP: TickDataProvider,
{
    let next_currency = if input_currency.equals(&pool.currency0) {
        &pool.currency1
    } else {
        &pool.currency0
    };
    (
        next_currency,
        PathKey {
            intermediateCurrency: if next_currency.is_native() {
                Address::ZERO
            } else {
                next_currency.address()
            },
            fee: U256::from(pool.fee),
            tickSpacing: pool.tick_spacing.to_i24(),
            hooks: pool.hooks,
            hookData: Bytes::default(),
        },
    )
}
