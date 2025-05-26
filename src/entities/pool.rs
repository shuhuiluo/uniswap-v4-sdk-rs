use crate::prelude::{Error, *};
use alloy_primitives::{aliases::U24, keccak256, uint, Address, ChainId, B256, I256, U160};
use alloy_sol_types::SolValue;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

pub const DYANMIC_FEE_FLAG: U24 = uint!(0x800000_U24);

/// Represents a V4 pool
#[derive(Clone, Debug)]
pub struct Pool<TP = NoTickDataProvider>
where
    TP: TickDataProvider,
{
    pub currency0: Currency,
    pub currency1: Currency,
    pub fee: U24,
    pub tick_spacing: TP::Index,
    pub sqrt_price_x96: U160,
    pub hooks: Address,
    pub liquidity: u128,
    pub tick_current: TP::Index,
    pub tick_data_provider: TP,
    pub pool_key: PoolKey,
    pub pool_id: B256,
}

impl<TP> PartialEq for Pool<TP>
where
    TP: TickDataProvider<Index: PartialEq>,
{
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.currency0 == other.currency0
            && self.currency1 == other.currency1
            && self.fee == other.fee
            && self.tick_spacing == other.tick_spacing
            && self.sqrt_price_x96 == other.sqrt_price_x96
            && self.hooks == other.hooks
            && self.liquidity == other.liquidity
            && self.tick_current == other.tick_current
    }
}

impl Pool {
    fn sort_currency(
        currency_a: &Currency,
        currency_b: &Currency,
    ) -> Result<(Address, Address), Error> {
        if currency_a.is_native() {
            Ok((Address::ZERO, currency_b.address()))
        } else if currency_b.is_native() {
            Ok((Address::ZERO, currency_a.address()))
        } else if sorts_before(currency_a, currency_b)? {
            Ok((currency_a.address(), currency_b.address()))
        } else {
            Ok((currency_b.address(), currency_a.address()))
        }
    }

    #[inline]
    pub fn get_pool_key<I: TickIndex>(
        currency_a: &Currency,
        currency_b: &Currency,
        fee: U24,
        tick_spacing: I,
        hooks: Address,
    ) -> Result<PoolKey, Error> {
        let (currency0_addr, currency1_addr) = Self::sort_currency(currency_a, currency_b)?;
        Ok(PoolKey {
            currency0: currency0_addr,
            currency1: currency1_addr,
            fee,
            tickSpacing: tick_spacing.to_i24(),
            hooks,
        })
    }

    #[inline]
    pub fn get_pool_id<I: TickIndex>(
        currency_a: &Currency,
        currency_b: &Currency,
        fee: U24,
        tick_spacing: I,
        hooks: Address,
    ) -> Result<B256, Error> {
        let (currency0_addr, currency1_addr) = Self::sort_currency(currency_a, currency_b)?;
        Ok(keccak256(
            (
                currency0_addr,
                currency1_addr,
                U24::from(fee),
                tick_spacing.to_i24(),
                hooks,
            )
                .abi_encode(),
        ))
    }

    /// Constructs a pool
    ///
    /// ## Arguments
    ///
    /// * `currency_a`: One of the currencies in the pool
    /// * `currency_b`: The other currency in the pool
    /// * `fee`: The fee in hundredths of a bips of the input amount of every swap that is collected
    ///   by the pool
    /// * `tick_spacing`: The tickSpacing of the pool
    /// * `hooks`: The address of the hook contract
    /// * `sqrt_price_x96`: The sqrt of the current ratio of amounts of currency1 to currency0
    /// * `liquidity`: The current value of in range liquidity
    #[inline]
    pub fn new(
        currency_a: Currency,
        currency_b: Currency,
        fee: U24,
        tick_spacing: <NoTickDataProvider as TickDataProvider>::Index,
        hooks: Address,
        sqrt_price_x96: U160,
        liquidity: u128,
    ) -> Result<Self, Error> {
        Self::new_with_tick_data_provider(
            currency_a,
            currency_b,
            fee,
            tick_spacing,
            hooks,
            sqrt_price_x96,
            liquidity,
            NoTickDataProvider,
        )
    }
}

impl<TP: TickDataProvider> Pool<TP> {
    /// Construct a pool with a tick data provider
    ///
    /// ## Arguments
    ///
    /// * `currency_a`: One of the currencies in the pool
    /// * `currency_b`: The other currency in the pool
    /// * `fee`: The fee in hundredths of a bips of the input amount of every swap that is collected
    ///   by the pool
    /// * `tick_spacing`: The tickSpacing of the pool
    /// * `hooks`: The address of the hook contract
    /// * `sqrt_price_x96`: The sqrt of the current ratio of amounts of currency1 to currency0
    /// * `liquidity`: The current value of in range liquidity
    /// * `tick_data_provider`: A tick data provider that can return tick data
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_tick_data_provider(
        currency_a: Currency,
        currency_b: Currency,
        fee: U24,
        tick_spacing: TP::Index,
        hooks: Address,
        sqrt_price_x96: U160,
        liquidity: u128,
        tick_data_provider: TP,
    ) -> Result<Self, Error> {
        assert!(fee == DYANMIC_FEE_FLAG || fee < uint!(1_000_000_U24), "FEE");
        if fee == DYANMIC_FEE_FLAG {
            assert_ne!(hooks, Address::ZERO, "Dynamic fee pool requires a hook");
        }
        let pool_key =
            Pool::get_pool_key(&currency_a, &currency_b, fee, tick_spacing.to_i24(), hooks)?;
        let pool_id = Pool::get_pool_id(&currency_a, &currency_b, fee, tick_spacing, hooks)?;
        let tick_current = TP::Index::from_i24(sqrt_price_x96.get_tick_at_sqrt_ratio()?);
        let (currency0, currency1) = if sorts_before(&currency_a, &currency_b)? {
            (currency_a, currency_b)
        } else {
            (currency_b, currency_a)
        };
        Ok(Self {
            currency0,
            currency1,
            fee,
            tick_spacing,
            sqrt_price_x96,
            hooks,
            liquidity,
            tick_current,
            tick_data_provider,
            pool_key,
            pool_id,
        })
    }

    #[inline]
    pub const fn token0(&self) -> &Currency {
        &self.currency0
    }

    #[inline]
    pub const fn token1(&self) -> &Currency {
        &self.currency1
    }

    /// Returns true if the currency is either currency0 or currency1
    ///
    /// ## Arguments
    ///
    /// * `currency`: The currency to check
    #[inline]
    pub fn involves_currency(&self, currency: &impl BaseCurrency) -> bool {
        self.currency0.equals(currency) || self.currency1.equals(currency)
    }

    #[inline]
    pub fn involves_token(&self, currency: &impl BaseCurrency) -> bool {
        self.involves_currency(currency)
    }

    /// v4-only involvesToken convenience method, used for mixed route ETH <-> WETH connection only
    #[inline]
    pub fn v4_involves_token(&self, currency: &impl BaseCurrency) -> bool {
        if self.involves_currency(currency) {
            return true;
        }
        let wrapped = currency.wrapped();
        wrapped.equals(&self.currency0)
            || wrapped.equals(&self.currency1)
            || wrapped.equals(self.currency0.wrapped())
            || wrapped.equals(self.currency1.wrapped())
    }

    /// Returns the current mid price of the pool in terms of currency0, i.e. the ratio of currency1
    /// over currency0
    #[inline]
    pub fn currency0_price(&self) -> Price<Currency, Currency> {
        let sqrt_price_x96 = self.sqrt_price_x96.to_big_int();
        Price::new(
            self.currency0.clone(),
            self.currency1.clone(),
            Q192.to_big_int(),
            sqrt_price_x96 * sqrt_price_x96,
        )
    }

    #[inline]
    pub fn token0_price(&self) -> Price<Currency, Currency> {
        self.currency0_price()
    }

    /// Returns the current mid price of the pool in terms of currency1, i.e. the ratio of currency0
    /// over currency1
    #[inline]
    pub fn currency1_price(&self) -> Price<Currency, Currency> {
        let sqrt_price_x96 = self.sqrt_price_x96.to_big_int();
        Price::new(
            self.currency1.clone(),
            self.currency0.clone(),
            sqrt_price_x96 * sqrt_price_x96,
            Q192.to_big_int(),
        )
    }

    #[inline]
    pub fn token1_price(&self) -> Price<Currency, Currency> {
        self.currency1_price()
    }

    /// Return the price of the given currency in terms of the other currency in the pool.
    ///
    /// ## Arguments
    ///
    /// * `currency`: The currency to return price of
    #[inline]
    pub fn price_of(
        &self,
        currency: &impl BaseCurrency,
    ) -> Result<Price<Currency, Currency>, Error> {
        if self.currency0.equals(currency) {
            Ok(self.currency0_price())
        } else if self.currency1.equals(currency) {
            Ok(self.currency1_price())
        } else {
            Err(Error::InvalidCurrency)
        }
    }

    /// Returns the chain ID of the currencies in the pool.
    #[inline]
    pub fn chain_id(&self) -> ChainId {
        self.currency0.chain_id()
    }

    /// Executes a swap
    ///
    /// ## Arguments
    ///
    /// * `zero_for_one`: Whether the amount in is token0 or token1
    /// * `amount_specified`: The amount of the swap, which implicitly configures the swap as exact
    ///   input (positive), or exact output (negative)
    /// * `sqrt_price_limit_x96`: The Q64.96 sqrt price limit. If zero for one, the price cannot be
    ///   less than this value after the swap. If one for zero, the price cannot be greater than
    ///   this value after the swap
    async fn swap(
        &self,
        zero_for_one: bool,
        amount_specified: I256,
        sqrt_price_limit_x96: Option<U160>,
    ) -> Result<SwapState<TP::Index>, Error> {
        if !self.hook_impacts_swap() {
            Ok(v3_swap(
                self.fee,
                self.sqrt_price_x96,
                self.tick_current,
                self.liquidity,
                self.tick_spacing,
                &self.tick_data_provider,
                zero_for_one,
                amount_specified,
                sqrt_price_limit_x96,
            )
            .await?)
        } else {
            Err(Error::UnsupportedHook)
        }
    }

    const fn hook_impacts_swap(&self) -> bool {
        // could use this function to clear certain hooks that may have swap Permissions, but we
        // know they don't interfere in the swap outcome
        has_swap_permissions(self.hooks)
    }
}

impl<TP: Clone + TickDataProvider> Pool<TP> {
    /// Given an input amount of a token, return the computed output amount, and a pool with state
    /// updated after the trade
    ///
    /// ## Note
    ///
    /// Works only for vanilla hookless v3 pools, otherwise throws an error
    ///
    /// ## Arguments
    ///
    /// * `input_amount`: The input amount for which to quote the output amount
    /// * `sqrt_price_limit_x96`: The Q64.96 sqrt price limit
    ///
    /// returns: The output amount and the pool with updated state
    #[inline]
    pub async fn get_output_amount(
        &self,
        input_amount: &CurrencyAmount<impl BaseCurrency>,
        sqrt_price_limit_x96: Option<U160>,
    ) -> Result<(CurrencyAmount<Currency>, Self), Error> {
        if !self.involves_currency(&input_amount.currency) {
            return Err(Error::InvalidCurrency);
        }

        let zero_for_one = input_amount.currency.equals(&self.currency0);

        let SwapState {
            amount_specified_remaining,
            amount_calculated: output_amount,
            sqrt_price_x96,
            liquidity,
            ..
        } = self
            .swap(
                zero_for_one,
                I256::from_big_int(input_amount.quotient()),
                sqrt_price_limit_x96,
            )
            .await?;

        if !amount_specified_remaining.is_zero() && sqrt_price_limit_x96.is_none() {
            return Err(Error::InsufficientLiquidity);
        }

        let output_currency = if zero_for_one {
            self.currency1.clone()
        } else {
            self.currency0.clone()
        };
        Ok((
            CurrencyAmount::from_raw_amount(output_currency, -output_amount.to_big_int())?,
            Self {
                sqrt_price_x96,
                tick_current: TP::Index::from_i24(sqrt_price_x96.get_tick_at_sqrt_ratio()?),
                liquidity,
                ..self.clone()
            },
        ))
    }

    /// Given a desired output amount of a currency, return the computed input amount and a pool
    /// with state updated after the trade
    ///
    /// ## Note
    ///
    /// Works only for vanilla hookless v3 pools, otherwise throws an error
    ///
    /// ## Arguments
    ///
    /// * `output_amount`: The output amount for which to quote the input amount
    /// * `sqrt_price_limit_x96`: The Q64.96 sqrt price limit. If zero for one, the price cannot be
    ///   less than this value after the swap. If one for zero, the price cannot be greater than
    ///   this value after the swap
    ///
    /// returns: The input amount and the pool with updated state
    #[inline]
    pub async fn get_input_amount(
        &self,
        output_amount: &CurrencyAmount<impl BaseCurrency>,
        sqrt_price_limit_x96: Option<U160>,
    ) -> Result<(CurrencyAmount<Currency>, Self), Error> {
        if !self.involves_currency(&output_amount.currency) {
            return Err(Error::InvalidCurrency);
        }

        let zero_for_one = output_amount.currency.equals(&self.currency1);

        let SwapState {
            amount_specified_remaining,
            amount_calculated: input_amount,
            sqrt_price_x96,
            liquidity,
            ..
        } = self
            .swap(
                zero_for_one,
                I256::from_big_int(-output_amount.quotient()),
                sqrt_price_limit_x96,
            )
            .await?;

        if !amount_specified_remaining.is_zero() && sqrt_price_limit_x96.is_none() {
            return Err(Error::InsufficientLiquidity);
        }

        let input_currency = if zero_for_one {
            self.currency0.clone()
        } else {
            self.currency1.clone()
        };
        Ok((
            CurrencyAmount::from_raw_amount(input_currency, input_amount.to_big_int())?,
            Self {
                sqrt_price_x96,
                tick_current: TP::Index::from_i24(sqrt_price_x96.get_tick_at_sqrt_ratio()?),
                liquidity,
                ..self.clone()
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{currency_amount, tests::*};
    use alloy_primitives::b256;

    mod constructor {
        use super::*;
        use alloy_primitives::address;

        #[test]
        #[should_panic(expected = "Core(ChainIdMismatch(1, 3))")]
        fn cannot_be_used_for_currencies_on_different_chains() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH9::on_chain(3).unwrap()),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        #[should_panic(expected = "FEE")]
        fn fee_cannot_be_more_than_1e6() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                uint!(1_000_000_U24),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        fn fee_can_be_dynamic() {
            let pool = Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                DYANMIC_FEE_FLAG,
                10,
                address!("fff0000000000000000000000000000000000000"),
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
            assert_eq!(pool.fee, DYANMIC_FEE_FLAG);
        }

        #[test]
        #[should_panic(expected = "Dynamic fee pool requires a hook")]
        fn dynamic_fee_pool_requires_hook() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                DYANMIC_FEE_FLAG,
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        #[should_panic(expected = "Core(EqualAddresses)")]
        fn cannot_be_given_two_of_the_same_currency() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(USDC.clone()),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        fn works_with_valid_arguments_for_empty_pool_medium_fee() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                FeeAmount::MEDIUM.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        fn works_with_valid_arguments_for_empty_pool_lowest_fee() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }

        #[test]
        fn works_with_valid_arguments_for_empty_pool_highest_fee() {
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(WETH.clone()),
                FeeAmount::HIGH.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();
        }
    }

    #[test]
    fn get_pool_id_returns_correct_pool_id() {
        let result1 = Pool::get_pool_id(
            &USDC.clone().into(),
            &DAI.clone().into(),
            FeeAmount::LOWEST.into(),
            10,
            Address::ZERO,
        )
        .unwrap();
        assert_eq!(
            result1,
            b256!("503fb8d73fd2351c645ae9fea85381bac6b16ea0c2038e14dc1e96d447c8ffbb")
        );

        let result2 = Pool::get_pool_id(
            &DAI.clone().into(),
            &USDC.clone().into(),
            FeeAmount::LOWEST.into(),
            10,
            Address::ZERO,
        )
        .unwrap();
        assert_eq!(result2, result1);
    }

    #[test]
    fn get_pool_key_returns_correct_pool_key() {
        let result1 = Pool::get_pool_key(
            &USDC.clone().into(),
            &DAI.clone().into(),
            FeeAmount::LOWEST.into(),
            10,
            Address::ZERO,
        )
        .unwrap();
        assert_eq!(
            result1,
            PoolKey {
                currency0: DAI.address(),
                currency1: USDC.address(),
                fee: FeeAmount::LOWEST.into(),
                tickSpacing: 10.to_i24(),
                hooks: Address::ZERO
            }
        );

        let result2 = Pool::get_pool_key(
            &DAI.clone().into(),
            &USDC.clone().into(),
            FeeAmount::LOWEST.into(),
            10,
            Address::ZERO,
        )
        .unwrap();
        assert_eq!(result2, result1);
    }

    #[test]
    fn currency0_always_is_the_currency_that_sorts_before() {
        assert_eq!(USDC_DAI.currency0, DAI.clone().into());
        assert_eq!(DAI_USDC.currency0, DAI.clone().into());
    }

    #[test]
    fn currency1_always_is_the_currency_that_sorts_after() {
        assert_eq!(USDC_DAI.currency1, USDC.clone().into());
        assert_eq!(DAI_USDC.currency1, USDC.clone().into());
    }

    #[test]
    fn pool_id_is_correct() {
        assert_eq!(
            USDC_DAI.pool_id,
            b256!("503fb8d73fd2351c645ae9fea85381bac6b16ea0c2038e14dc1e96d447c8ffbb")
        );
    }

    #[test]
    fn pool_key_is_correct() {
        assert_eq!(
            USDC_DAI.pool_key,
            PoolKey {
                currency0: DAI.address(),
                currency1: USDC.address(),
                fee: FeeAmount::LOWEST.into(),
                tickSpacing: 10.to_i24(),
                hooks: Address::ZERO
            }
        );
    }

    #[test]
    fn currency0_price_returns_price_of_currency0_in_terms_of_currency1() {
        assert_eq!(
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(DAI.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(BigInt::from(101e6 as u128), BigInt::from(100e18 as u128)),
                0,
            )
            .unwrap()
            .currency0_price()
            .to_significant(5, None)
            .unwrap(),
            "1.01"
        );
        assert_eq!(
            Pool::new(
                Currency::Token(DAI.clone()),
                Currency::Token(USDC.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(BigInt::from(101e6 as u128), BigInt::from(100e18 as u128)),
                0,
            )
            .unwrap()
            .currency0_price()
            .to_significant(5, None)
            .unwrap(),
            "1.01"
        );
    }

    #[test]
    fn currency1_price_returns_price_of_currency1_in_terms_of_currency0() {
        assert_eq!(
            Pool::new(
                Currency::Token(USDC.clone()),
                Currency::Token(DAI.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(BigInt::from(101e6 as u128), BigInt::from(100e18 as u128)),
                0,
            )
            .unwrap()
            .currency1_price()
            .to_significant(5, None)
            .unwrap(),
            "0.9901"
        );
        assert_eq!(
            Pool::new(
                Currency::Token(DAI.clone()),
                Currency::Token(USDC.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                encode_sqrt_ratio_x96(BigInt::from(101e6 as u128), BigInt::from(100e18 as u128)),
                0,
            )
            .unwrap()
            .currency1_price()
            .to_significant(5, None)
            .unwrap(),
            "0.9901"
        );
    }

    mod price_of {
        use super::*;

        #[test]
        fn returns_price_of_currency_in_terms_of_other_currency() {
            assert_eq!(
                USDC_DAI.price_of(&DAI.clone()).unwrap(),
                USDC_DAI.currency0_price()
            );
            assert_eq!(
                USDC_DAI.price_of(&USDC.clone()).unwrap(),
                USDC_DAI.currency1_price()
            );
        }

        #[test]
        #[should_panic(expected = "InvalidCurrency")]
        fn throws_if_invalid_currency() {
            USDC_DAI.price_of(&WETH.clone()).unwrap();
        }
    }

    #[test]
    fn chain_id_returns_chain_id_of_currencies() {
        assert_eq!(USDC_DAI.chain_id(), 1);
        assert_eq!(DAI_USDC.chain_id(), 1);
    }

    #[test]
    fn involves_currency_returns_true_if_currency_is_in_pool() {
        assert!(USDC_DAI.involves_currency(&USDC.clone()));
        assert!(USDC_DAI.involves_currency(&DAI.clone()));
        assert!(!USDC_DAI.involves_currency(&WETH9::on_chain(1).unwrap()));
    }

    mod v4_involves_token {
        use super::*;

        #[test]
        fn pool_with_native_eth_and_dai() {
            let pool = Pool::new(
                ETHER.clone().into(),
                DAI.clone().into(),
                FeeAmount::LOW.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();

            assert!(pool.v4_involves_token(&ETHER.clone()));
            assert!(pool.v4_involves_token(&DAI.clone()));
            assert!(pool.v4_involves_token(&WETH.clone()));
        }

        #[test]
        fn pool_with_weth_and_dai() {
            let pool = Pool::new(
                WETH.clone().into(),
                DAI.clone().into(),
                FeeAmount::LOW.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                0,
            )
            .unwrap();

            assert!(pool.v4_involves_token(&ETHER.clone()));
            assert!(pool.v4_involves_token(&DAI.clone()));
            assert!(pool.v4_involves_token(&WETH.clone()));
        }
    }

    mod swaps {
        use super::*;
        use once_cell::sync::Lazy;

        static POOL: Lazy<Pool<Vec<Tick>>> = Lazy::new(|| {
            Pool::new_with_tick_data_provider(
                Currency::Token(USDC.clone()),
                Currency::Token(DAI.clone()),
                FeeAmount::LOWEST.into(),
                10,
                Address::ZERO,
                *SQRT_PRICE_1_1,
                ONE_ETHER,
                TICK_LIST.clone(),
            )
            .unwrap()
        });

        mod get_output_amount {
            use super::*;

            #[tokio::test]
            async fn usdc_to_dai() {
                let input_amount = currency_amount!(USDC, 100);
                let (output_amount, _) = POOL.get_output_amount(&input_amount, None).await.unwrap();
                assert!(output_amount.currency.equals(&DAI.clone()));
                assert_eq!(output_amount.quotient(), 98.into());
            }

            #[tokio::test]
            async fn dai_to_usdc() {
                let input_amount = currency_amount!(DAI, 100);
                let (output_amount, _) = POOL.get_output_amount(&input_amount, None).await.unwrap();
                assert!(output_amount.currency.equals(&USDC.clone()));
                assert_eq!(output_amount.quotient(), 98.into());
            }
        }

        mod get_input_amount {
            use super::*;

            #[tokio::test]
            async fn usdc_to_dai() {
                let output_amount = currency_amount!(DAI, 98);
                let (input_amount, _) = POOL.get_input_amount(&output_amount, None).await.unwrap();
                assert!(input_amount.currency.equals(&USDC.clone()));
                assert_eq!(input_amount.quotient(), 100.into());
            }

            #[tokio::test]
            async fn dai_to_usdc() {
                let output_amount = currency_amount!(USDC, 98);
                let (input_amount, _) = POOL.get_input_amount(&output_amount, None).await.unwrap();
                assert!(input_amount.currency.equals(&DAI.clone()));
                assert_eq!(input_amount.quotient(), 100.into());
            }
        }
    }
}
