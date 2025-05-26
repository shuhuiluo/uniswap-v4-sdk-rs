use crate::prelude::{PathKey, Pool, Route};
use alloy_primitives::{Address, Bytes, U256};
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_route, tests::*};
    use alloy_primitives::{aliases::I24, uint};
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::token;

    static CURRENCY1: Lazy<Token> =
        Lazy::new(|| token!(1, "1111111111111111111111111111111111111111", 18, "t1"));
    static CURRENCY2: Lazy<Token> =
        Lazy::new(|| token!(1, "2222222222222222222222222222222222222222", 18, "t2"));
    static CURRENCY3: Lazy<Token> =
        Lazy::new(|| token!(1, "3333333333333333333333333333333333333333", 18, "t3"));
    static POOL_ETH_1: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            ETHER.clone().into(),
            CURRENCY1.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });
    static POOL_1_2: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY1.clone().into(),
            CURRENCY2.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });
    static POOL_2_3: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY2.clone().into(),
            CURRENCY3.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });
    static ROUTE: Lazy<Route<Ether, Currency, NoTickDataProvider>> = Lazy::new(
        || create_route!(POOL_ETH_1, POOL_1_2, POOL_2_3; ETHER, Currency::from(CURRENCY3.clone())),
    );

    #[test]
    fn test_encodes_correct_route_for_exact_in() {
        let expected = vec![
            PathKey {
                intermediateCurrency: CURRENCY1.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY2.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY3.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
        ];

        assert_eq!(encode_route_to_path(&ROUTE, false), expected);
    }

    #[test]
    fn test_encodes_correct_route_for_exact_out() {
        let expected = vec![
            PathKey {
                intermediateCurrency: Address::ZERO,
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY1.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY2.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
        ];

        assert_eq!(encode_route_to_path(&ROUTE, true), expected);
    }

    #[test]
    fn test_encodes_correct_path_when_route_has_different_output_than_route_path_output() {
        let new_route = create_route!(POOL_1_2, POOL_ETH_1; CURRENCY2, WETH);
        let exact_output = true;
        let expected = vec![
            PathKey {
                intermediateCurrency: CURRENCY2.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY1.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
        ];

        assert_eq!(encode_route_to_path(&new_route, exact_output), expected);
    }

    #[test]
    fn test_encodes_correct_path_when_route_has_different_input_than_route_path_input() {
        let new_route = create_route!(POOL_ETH_1, POOL_1_2; WETH, CURRENCY2);
        let exact_output = false;
        let expected = vec![
            PathKey {
                intermediateCurrency: CURRENCY1.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
            PathKey {
                intermediateCurrency: CURRENCY2.address(),
                fee: uint!(3000_U256),
                tickSpacing: I24::unchecked_from(10),
                hooks: Address::ZERO,
                hookData: Bytes::default(),
            },
        ];

        assert_eq!(encode_route_to_path(&new_route, exact_output), expected);
    }
}
