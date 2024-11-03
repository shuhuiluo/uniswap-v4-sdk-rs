use crate::prelude::{Error, *};
use alloy_primitives::{
    aliases::{I24, U24},
    keccak256, uint, Address, ChainId, B256, I256, U160,
};
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
    pub sqrt_ratio_x96: U160,
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
            && self.sqrt_ratio_x96 == other.sqrt_ratio_x96
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
    pub fn get_pool_key(
        currency_a: &Currency,
        currency_b: &Currency,
        fee: U24,
        tick_spacing: I24,
        hooks: Address,
    ) -> Result<PoolKey, Error> {
        let (currency0_addr, currency1_addr) = Self::sort_currency(currency_a, currency_b)?;
        Ok(PoolKey {
            currency0: currency0_addr,
            currency1: currency1_addr,
            fee,
            tickSpacing: tick_spacing,
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
    /// * `sqrt_ratio_x96`: The sqrt of the current ratio of amounts of currency1 to currency0
    /// * `liquidity`: The current value of in range liquidity
    #[inline]
    pub fn new(
        currency_a: Currency,
        currency_b: Currency,
        fee: U24,
        tick_spacing: <NoTickDataProvider as TickDataProvider>::Index,
        hooks: Address,
        sqrt_ratio_x96: U160,
        liquidity: u128,
    ) -> Result<Self, Error> {
        Self::new_with_tick_data_provider(
            currency_a,
            currency_b,
            fee,
            tick_spacing,
            hooks,
            sqrt_ratio_x96,
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
    /// * `sqrt_ratio_x96`: The sqrt of the current ratio of amounts of currency1 to currency0
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
        sqrt_ratio_x96: U160,
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
        let tick_current = sqrt_ratio_x96
            .get_tick_at_sqrt_ratio()?
            .as_i32()
            .try_into()
            .unwrap();
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
            sqrt_ratio_x96,
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

    /// Returns the current mid price of the pool in terms of currency0, i.e. the ratio of currency1
    /// over currency0
    #[inline]
    pub fn currency0_price(&self) -> Price<Currency, Currency> {
        let sqrt_ratio_x96 = self.sqrt_ratio_x96.to_big_uint();
        Price::new(
            self.currency0.clone(),
            self.currency1.clone(),
            Q192.to_big_int(),
            &sqrt_ratio_x96 * &sqrt_ratio_x96,
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
        let sqrt_ratio_x96 = self.sqrt_ratio_x96.to_big_uint();
        Price::new(
            self.currency1.clone(),
            self.currency0.clone(),
            &sqrt_ratio_x96 * &sqrt_ratio_x96,
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
    fn swap(
        &self,
        zero_for_one: bool,
        amount_specified: I256,
        sqrt_price_limit_x96: Option<U160>,
    ) -> Result<SwapState<TP::Index>, Error> {
        if self.non_impactful_hook() {
            Ok(v3_swap(
                self.fee,
                self.sqrt_ratio_x96,
                self.tick_current,
                self.liquidity,
                self.tick_spacing,
                &self.tick_data_provider,
                zero_for_one,
                amount_specified,
                sqrt_price_limit_x96,
            )?)
        } else {
            Err(Error::UnsupportedHook)
        }
    }

    fn non_impactful_hook(&self) -> bool {
        self.hooks == Address::ZERO
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
    pub fn get_output_amount(
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
        } = self.swap(
            zero_for_one,
            I256::from_big_int(input_amount.quotient()),
            sqrt_price_limit_x96,
        )?;

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
            Self::new_with_tick_data_provider(
                self.currency0.clone(),
                self.currency1.clone(),
                self.fee,
                self.tick_spacing,
                self.hooks,
                sqrt_price_x96,
                liquidity,
                self.tick_data_provider.clone(),
            )?,
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
    pub fn get_input_amount(
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
        } = self.swap(
            zero_for_one,
            I256::from_big_int(-output_amount.quotient()),
            sqrt_price_limit_x96,
        )?;

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
            Self::new_with_tick_data_provider(
                self.currency0.clone(),
                self.currency1.clone(),
                self.fee,
                self.tick_spacing,
                self.hooks,
                sqrt_price_x96,
                liquidity,
                self.tick_data_provider.clone(),
            )?,
        ))
    }
}
