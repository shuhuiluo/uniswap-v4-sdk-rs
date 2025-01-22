use crate::prelude::*;
use alloc::vec::Vec;
use alloy_primitives::ChainId;
use uniswap_sdk_core::prelude::{BaseCurrency, Currency, Price};
use uniswap_v3_sdk::entities::TickDataProvider;

/// Represents a list of pools through which a swap can occur
#[derive(Clone, PartialEq, Debug)]
pub struct Route<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    pub pools: Vec<Pool<TP>>,
    /// The input currency
    pub input: TInput,
    /// The output currency
    pub output: TOutput,
    /// equivalent or wrapped/unwrapped input to match pool
    pub path_input: Currency,
    /// equivalent or wrapped/unwrapped output to match pool
    pub path_output: Currency,
    _mid_price: Option<Price<TInput, TOutput>>,
}

impl<TInput, TOutput, TP> Route<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    /// Creates an instance of route.
    ///
    /// ## Arguments
    ///
    /// * `pools`: An array of [`Pool`] objects, ordered by the route the swap will take
    /// * `input`: The input currency
    /// * `output`: The output currency
    #[inline]
    pub fn new(pools: Vec<Pool<TP>>, input: TInput, output: TOutput) -> Result<Self, Error> {
        assert!(!pools.is_empty(), "POOLS");

        let chain_id = pools[0].chain_id();
        let all_on_same_chain = pools.iter().all(|pool| pool.chain_id() == chain_id);
        assert!(all_on_same_chain, "CHAIN_IDS");

        // throws if pools do not involve the input and output currency or the native/wrapped
        // equivalent
        let path_input = get_path_currency(&input, &pools[0])?;
        let path_output = get_path_currency(&output, pools.last().unwrap())?;

        let mut current_input_currency = &path_input;
        for pool in &pools {
            current_input_currency = if current_input_currency.equals(&pool.currency0) {
                &pool.currency1
            } else if current_input_currency.equals(&pool.currency1) {
                &pool.currency0
            } else {
                panic!("PATH")
            };
        }
        assert!(current_input_currency.equals(&path_output), "PATH");

        Ok(Self {
            pools,
            input,
            output,
            path_input,
            path_output,
            _mid_price: None,
        })
    }

    /// Normalizes currency0-currency1 order and selects the next currency/fee step to add to the
    /// path
    #[inline]
    pub fn currency_path(&self) -> Vec<Currency> {
        let mut currency_path: Vec<Currency> = Vec::with_capacity(self.pools.len() + 1);
        currency_path.push(self.path_input.clone());
        for (i, pool) in self.pools.iter().enumerate() {
            let next_currency = if currency_path[i].equals(&pool.currency0) {
                pool.currency1.clone()
            } else {
                pool.currency0.clone()
            };
            currency_path.push(next_currency);
        }
        currency_path
    }

    #[inline]
    pub fn chain_id(&self) -> ChainId {
        self.pools[0].chain_id()
    }

    /// Returns the mid price of the route
    #[inline]
    pub fn mid_price(&self) -> Result<Price<TInput, TOutput>, Error> {
        let mut price = self.pools[0].price_of(&self.path_input)?;
        for pool in &self.pools[1..] {
            price = price.multiply(&pool.price_of(&price.quote_currency)?)?;
        }
        Ok(Price::new(
            self.input.clone(),
            self.output.clone(),
            price.denominator,
            price.numerator,
        ))
    }

    /// Returns the cached mid price of the route
    #[inline]
    pub fn mid_price_cached(&mut self) -> Result<Price<TInput, TOutput>, Error> {
        if let Some(mid_price) = &self._mid_price {
            return Ok(mid_price.clone());
        }
        let mid_price = self.mid_price()?;
        self._mid_price = Some(mid_price.clone());
        Ok(mid_price)
    }
}

#[cfg(test)]
mod tests {
    use super::{Pool, Route};
    use crate::tests::*;
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::{prelude::*, token};
    use uniswap_v3_sdk::prelude::*;

    static CURRENCY0: Lazy<Currency> =
        Lazy::new(|| token!(1, "0000000000000000000000000000000000000001", 18, "t0").into());
    static CURRENCY1: Lazy<Currency> =
        Lazy::new(|| token!(1, "0000000000000000000000000000000000000002", 18, "t1").into());
    static CURRENCY2: Lazy<Currency> =
        Lazy::new(|| token!(1, "0000000000000000000000000000000000000003", 18, "t2").into());
    static POOL_0_1: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY0.clone(),
            CURRENCY1.clone(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });
    static POOL_0_ETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY0.clone(),
            ETHER.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });
    static POOL_1_ETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY1.clone(),
            ETHER.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });
    static POOL_0_WETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY0.clone(),
            WETH.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });
    static POOL_ETH_WETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            ETHER.clone().into(),
            WETH.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });

    mod path {
        use super::*;

        #[test]
        fn constructs_a_path_from_the_currencies() {
            let route =
                Route::new(vec![POOL_0_1.clone()], CURRENCY0.clone(), CURRENCY1.clone()).unwrap();
            assert_eq!(route.pools, vec![POOL_0_1.clone()]);
            assert_eq!(
                route.currency_path(),
                vec![CURRENCY0.clone(), CURRENCY1.clone()]
            );
            assert_eq!(route.input, CURRENCY0.clone());
            assert_eq!(route.output, CURRENCY1.clone());
            assert_eq!(route.chain_id(), 1);
        }

        #[test]
        #[should_panic(expected = "InvalidCurrency")]
        fn should_fail_if_the_input_is_not_in_the_first_pool() {
            Route::new(vec![POOL_0_1.clone()], ETHER.clone(), CURRENCY1.clone()).unwrap();
        }

        #[test]
        #[should_panic(expected = "InvalidCurrency")]
        fn should_fail_if_the_output_is_not_in_the_last_pool() {
            Route::new(vec![POOL_0_1.clone()], CURRENCY0.clone(), ETHER.clone()).unwrap();
        }
    }

    #[test]
    fn can_have_a_currency_as_both_input_and_output() {
        let route = Route::new(
            vec![POOL_0_ETH.clone(), POOL_0_1.clone(), POOL_1_ETH.clone()],
            ETHER.clone(),
            ETHER.clone(),
        )
        .unwrap();
        assert_eq!(
            route.pools,
            vec![POOL_0_ETH.clone(), POOL_0_1.clone(), POOL_1_ETH.clone()]
        );
        assert_eq!(route.input, ETHER.clone());
        assert_eq!(route.output, ETHER.clone());
    }

    #[test]
    fn supports_ether_input() {
        let route = Route::new(vec![POOL_0_ETH.clone()], ETHER.clone(), CURRENCY0.clone()).unwrap();
        assert_eq!(route.pools, vec![POOL_0_ETH.clone()]);
        assert_eq!(route.input, ETHER.clone());
        assert_eq!(route.output, CURRENCY0.clone());
    }

    #[test]
    fn supports_ether_output() {
        let route = Route::new(vec![POOL_0_ETH.clone()], CURRENCY0.clone(), ETHER.clone()).unwrap();
        assert_eq!(route.pools, vec![POOL_0_ETH.clone()]);
        assert_eq!(route.input, CURRENCY0.clone());
        assert_eq!(route.output, ETHER.clone());
    }

    #[test]
    #[should_panic(expected = "PATH")]
    fn does_not_support_weth_to_eth_conversion_without_trading_through_an_eth_to_weth_pool() {
        Route::new(
            vec![POOL_0_WETH.clone(), POOL_1_ETH.clone()],
            CURRENCY0.clone(),
            CURRENCY1.clone(),
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "PATH")]
    fn does_not_support_eth_to_weth_conversion_without_trading_through_an_eth_to_weth_pool() {
        Route::new(
            vec![POOL_1_ETH.clone(), POOL_0_WETH.clone()],
            CURRENCY1.clone(),
            CURRENCY0.clone(),
        )
        .unwrap();
    }

    #[test]
    fn supports_trading_through_eth_weth_pools() {
        let route = Route::new(
            vec![
                POOL_0_WETH.clone(),
                POOL_ETH_WETH.clone(),
                POOL_1_ETH.clone(),
            ],
            CURRENCY0.clone(),
            CURRENCY1.clone(),
        )
        .unwrap();
        assert_eq!(
            route.pools,
            vec![
                POOL_0_WETH.clone(),
                POOL_ETH_WETH.clone(),
                POOL_1_ETH.clone()
            ]
        );
        assert_eq!(route.input, CURRENCY0.clone());
        assert_eq!(route.output, CURRENCY1.clone());
    }

    mod mid_price {
        use super::*;

        static POOL_0_1: Lazy<Pool> = Lazy::new(|| {
            Pool::new(
                CURRENCY0.clone(),
                CURRENCY1.clone(),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(1, 5),
                0,
            )
            .unwrap()
        });
        static POOL_1_2: Lazy<Pool> = Lazy::new(|| {
            Pool::new(
                CURRENCY1.clone(),
                CURRENCY2.clone(),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(15, 30),
                0,
            )
            .unwrap()
        });
        static POOL_0_ETH: Lazy<Pool> = Lazy::new(|| {
            Pool::new(
                CURRENCY0.clone(),
                ETHER.clone().into(),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(3, 1),
                0,
            )
            .unwrap()
        });
        static POOL_1_ETH: Lazy<Pool> = Lazy::new(|| {
            Pool::new(
                CURRENCY1.clone(),
                ETHER.clone().into(),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(1, 7),
                0,
            )
            .unwrap()
        });

        #[test]
        fn correct_for_0_to_1() {
            let route =
                Route::new(vec![POOL_0_1.clone()], CURRENCY0.clone(), CURRENCY1.clone()).unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "0.2000");
            assert!(price.base_currency.equals(&CURRENCY0.clone()));
            assert!(price.quote_currency.equals(&CURRENCY1.clone()));
        }

        #[test]
        fn is_cached() {
            let mut route =
                Route::new(vec![POOL_0_1.clone()], CURRENCY0.clone(), CURRENCY1.clone()).unwrap();
            let mid_price = route.mid_price_cached().unwrap();
            assert_eq!(mid_price, route.mid_price_cached().unwrap());
        }

        #[test]
        fn correct_for_1_to_0() {
            let route =
                Route::new(vec![POOL_0_1.clone()], CURRENCY1.clone(), CURRENCY0.clone()).unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "5.0000");
            assert!(price.base_currency.equals(&CURRENCY1.clone()));
            assert!(price.quote_currency.equals(&CURRENCY0.clone()));
        }

        #[test]
        fn correct_for_0_to_1_to_2() {
            let route = Route::new(
                vec![POOL_0_1.clone(), POOL_1_2.clone()],
                CURRENCY0.clone(),
                CURRENCY2.clone(),
            )
            .unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "0.1000");
            assert!(price.base_currency.equals(&CURRENCY0.clone()));
            assert!(price.quote_currency.equals(&CURRENCY2.clone()));
        }

        #[test]
        fn correct_for_2_to_1_to_0() {
            let route = Route::new(
                vec![POOL_1_2.clone(), POOL_0_1.clone()],
                CURRENCY2.clone(),
                CURRENCY0.clone(),
            )
            .unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "10.0000");
            assert!(price.base_currency.equals(&CURRENCY2.clone()));
            assert!(price.quote_currency.equals(&CURRENCY0.clone()));
        }

        #[test]
        fn correct_for_ether_to_0() {
            let route =
                Route::new(vec![POOL_0_ETH.clone()], ETHER.clone(), CURRENCY0.clone()).unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "3.0000");
            assert!(price.base_currency.equals(&ETHER.clone()));
            assert!(price.quote_currency.equals(&CURRENCY0.clone()));
        }

        #[test]
        fn correct_for_1_to_eth() {
            let route =
                Route::new(vec![POOL_1_ETH.clone()], CURRENCY1.clone(), ETHER.clone()).unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_fixed(4, None), "7.0000");
            assert!(price.base_currency.equals(&CURRENCY1.clone()));
            assert!(price.quote_currency.equals(&ETHER.clone()));
        }

        #[test]
        fn correct_for_ether_to_0_to_1_to_eth() {
            let route = Route::new(
                vec![POOL_0_ETH.clone(), POOL_0_1.clone(), POOL_1_ETH.clone()],
                ETHER.clone(),
                ETHER.clone(),
            )
            .unwrap();
            let price = route.mid_price().unwrap();
            assert_eq!(price.to_significant(4, None).unwrap(), "4.2");
            assert!(price.base_currency.equals(&ETHER.clone()));
            assert!(price.quote_currency.equals(&ETHER.clone()));
        }

        #[test]
        fn can_be_constructed_with_ether_as_input_on_a_weth_pool() {
            let route =
                Route::new(vec![POOL_0_WETH.clone()], ETHER.clone(), CURRENCY0.clone()).unwrap();
            assert_eq!(route.input, ETHER.clone());
            assert_eq!(route.path_input, WETH.clone().into());
            assert_eq!(route.output, CURRENCY0.clone());
            assert_eq!(route.path_output, CURRENCY0.clone());
        }

        #[test]
        fn can_be_constructed_with_weth_as_input_on_a_eth_pool() {
            let route =
                Route::new(vec![POOL_0_ETH.clone()], WETH.clone(), CURRENCY0.clone()).unwrap();
            assert_eq!(route.input, WETH.clone());
            assert_eq!(route.path_input, ETHER.clone().into());
            assert_eq!(route.output, CURRENCY0.clone());
            assert_eq!(route.path_output, CURRENCY0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output_on_a_weth_pool() {
            let route =
                Route::new(vec![POOL_0_WETH.clone()], CURRENCY0.clone(), ETHER.clone()).unwrap();
            assert_eq!(route.input, CURRENCY0.clone());
            assert_eq!(route.path_input, CURRENCY0.clone());
            assert_eq!(route.output, ETHER.clone());
            assert_eq!(route.path_output, WETH.clone().into());
        }

        #[test]
        fn can_be_constructed_with_weth_as_output_on_a_eth_pool() {
            let route =
                Route::new(vec![POOL_0_ETH.clone()], CURRENCY0.clone(), WETH.clone()).unwrap();
            assert_eq!(route.input, CURRENCY0.clone());
            assert_eq!(route.path_input, CURRENCY0.clone());
            assert_eq!(route.output, WETH.clone());
            assert_eq!(route.path_output, ETHER.clone().into());
        }
    }
}
