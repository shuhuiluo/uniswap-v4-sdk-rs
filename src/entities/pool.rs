use crate::prelude::{Error, *};
use alloy_primitives::{Address, B256, ChainId, I256, U160, aliases::U24, keccak256, uint};
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
    /// PoolKey fee (the dynamic flag for dynamic-fee pools).
    pub fee: U24,
    /// Current LP fee used for quotes, unresolved until loaded for dynamic pools.
    pub lp_fee: Option<U24>,
    /// Packed directional protocol fees: low 12 bits for zero-for-one.
    pub protocol_fee: U24,
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
            && self.lp_fee == other.lp_fee
            && self.protocol_fee == other.protocol_fee
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
        assert!(
            fee == DYANMIC_FEE_FLAG || fee <= uint!(1_000_000_U24),
            "FEE"
        );
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
            lp_fee: (fee != DYANMIC_FEE_FLAG).then_some(fee),
            protocol_fee: U24::ZERO,
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

    /// Set the current LP fee and packed directional protocol fees without changing pool identity.
    #[inline]
    pub fn with_fees(mut self, lp_fee: U24, protocol_fee: U24) -> Result<Self, Error> {
        let packed = protocol_fee.to::<u32>();
        if (self.fee != DYANMIC_FEE_FLAG && lp_fee != self.fee)
            || lp_fee > uint!(1_000_000_U24)
            || packed & 0xfff > 1000
            || packed >> 12 > 1000
        {
            return Err(Error::InvalidFee);
        }
        self.lp_fee = Some(lp_fee);
        self.protocol_fee = protocol_fee;
        Ok(self)
    }

    /// The total input fee, applying protocol fees before LP fees as in v4-core.
    #[inline]
    pub fn swap_fee(&self, zero_for_one: bool) -> Result<U24, Error> {
        let lp_fee = self.lp_fee.ok_or(Error::UnresolvedDynamicFee)?.to::<u32>();
        let packed = self.protocol_fee.to::<u32>();
        let protocol_fee = if zero_for_one {
            packed & 0xfff
        } else {
            packed >> 12
        };
        if (self.fee != DYANMIC_FEE_FLAG && lp_fee != self.fee.to::<u32>())
            || lp_fee > 1_000_000
            || packed & 0xfff > 1000
            || packed >> 12 > 1000
        {
            return Err(Error::InvalidFee);
        }
        Ok(U24::from(
            protocol_fee + lp_fee - protocol_fee * lp_fee / 1_000_000,
        ))
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
            let fee = self.swap_fee(zero_for_one)?;
            if fee == uint!(1_000_000_U24) {
                return self
                    .swap_full_fee(zero_for_one, amount_specified, sqrt_price_limit_x96)
                    .await;
            }
            Ok(v3_swap(
                fee,
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

    // A 100% fee cannot use v3's step math, which divides by the fee complement.
    // Tick traversal follows uniswap-v3-sdk's MIT-licensed v3_swap implementation.
    async fn swap_full_fee(
        &self,
        zero_for_one: bool,
        amount_specified: I256,
        sqrt_price_limit_x96: Option<U160>,
    ) -> Result<SwapState<TP::Index>, Error> {
        if amount_specified < I256::ZERO {
            return Err(Error::InvalidFeeForExactOutput);
        }
        // Reuse the shared price-limit validation without taking a swap step.
        let mut state = v3_swap(
            U24::ZERO,
            self.sqrt_price_x96,
            self.tick_current,
            self.liquidity,
            self.tick_spacing,
            &self.tick_data_provider,
            zero_for_one,
            I256::ZERO,
            sqrt_price_limit_x96,
        )
        .await?;
        state.amount_specified_remaining = amount_specified;
        let limit = sqrt_price_limit_x96.unwrap_or(if zero_for_one {
            MIN_SQRT_RATIO + U160::from(1)
        } else {
            MAX_SQRT_RATIO - U160::from(1)
        });
        while !state.amount_specified_remaining.is_zero() && state.sqrt_price_x96 != limit {
            let (tick, initialized) = self
                .tick_data_provider
                .next_initialized_tick_within_one_word(
                    state.tick_current,
                    zero_for_one,
                    self.tick_spacing,
                )
                .await?;
            let tick = TP::Index::from_i24(tick.to_i24().clamp(MIN_TICK, MAX_TICK));
            let next = get_sqrt_ratio_at_tick(tick.to_i24())?;
            let target = if zero_for_one {
                next.max(limit)
            } else {
                next.min(limit)
            };
            let amount_in = if zero_for_one {
                get_amount_0_delta(target, state.sqrt_price_x96, state.liquidity, true)?
            } else {
                get_amount_1_delta(state.sqrt_price_x96, target, state.liquidity, true)?
            };
            if !amount_in.is_zero() {
                // No input remains after the fee; price stays put and the entire input is fee.
                state.amount_specified_remaining = I256::ZERO;
                break;
            }
            state.sqrt_price_x96 = target;
            if target == next {
                if initialized {
                    let liquidity_net = self.tick_data_provider.get_tick(tick).await?.liquidity_net;
                    state.liquidity = add_delta(
                        state.liquidity,
                        if zero_for_one {
                            -liquidity_net
                        } else {
                            liquidity_net
                        },
                    )?;
                }
                state.tick_current = if zero_for_one {
                    tick - TP::Index::ONE
                } else {
                    tick
                };
            } else {
                state.tick_current = TP::Index::from_i24(target.get_tick_at_sqrt_ratio()?);
            }
        }
        Ok(state)
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
            tick_current,
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
                tick_current,
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
            tick_current,
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
                tick_current,
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

    #[tokio::test]
    async fn issue74_protocol_fee_and_dynamic_fee() {
        let pool = Pool::new_with_tick_data_provider(
            TOKEN0.clone().into(),
            TOKEN1.clone().into(),
            uint!(3000_U24),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            1000 * ONE_ETHER,
            vec![
                Tick {
                    index: -887270,
                    liquidity_net: (1000 * ONE_ETHER) as i128,
                    liquidity_gross: 1000 * ONE_ETHER,
                },
                Tick {
                    index: 887270,
                    liquidity_net: -((1000 * ONE_ETHER) as i128),
                    liquidity_gross: 1000 * ONE_ETHER,
                },
            ],
        )
        .unwrap();
        let identity = pool.pool_id;
        let pool = pool
            .with_fees(uint!(3000_U24), U24::from(1000 + (500 << 12)))
            .unwrap();
        assert_eq!(pool.pool_id, identity);
        assert_eq!(pool.swap_fee(true).unwrap(), uint!(3997_U24));
        assert_eq!(pool.swap_fee(false).unwrap(), uint!(3499_U24));
        let (output, _) = pool
            .get_output_amount(&currency_amount!(TOKEN0, ONE_ETHER), None)
            .await
            .unwrap();
        assert_eq!(
            output.quotient(),
            BigInt::from_u128(995011965097726768_u128).unwrap()
        );
        let mut dynamic = pool.clone();
        dynamic.fee = DYANMIC_FEE_FLAG;
        dynamic.lp_fee = None;
        assert!(matches!(
            dynamic.swap_fee(true),
            Err(Error::UnresolvedDynamicFee)
        ));
        assert_eq!(
            dynamic
                .with_fees(uint!(3000_U24), uint!(1000_U24))
                .unwrap()
                .swap_fee(true)
                .unwrap(),
            uint!(3997_U24)
        );
        assert!(
            pool.clone()
                .with_fees(uint!(3000_U24), uint!(1001_U24))
                .is_err()
        );
        assert!(pool.with_fees(uint!(1_000_001_U24), U24::ZERO).is_err());
    }

    #[tokio::test]
    async fn issue74_boundary_tick_is_preserved() {
        let mut ticks = TICK_LIST.clone();
        ticks.insert(
            1,
            Tick {
                index: -10,
                liquidity_net: 0,
                liquidity_gross: 1,
            },
        );
        let pool = Pool::new_with_tick_data_provider(
            TOKEN0.clone().into(),
            TOKEN1.clone().into(),
            uint!(3000_U24),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            ONE_ETHER,
            ticks,
        )
        .unwrap();
        let limit = get_sqrt_ratio_at_tick((-10_i32).to_i24()).unwrap();
        let (_, quoted) = pool
            .get_output_amount(&currency_amount!(TOKEN0, ONE_ETHER), Some(limit))
            .await
            .unwrap();
        assert_eq!(quoted.tick_current, -11);
    }

    fn full_fee_test_pool(liquidity: u128, ticks: Vec<Tick>) -> Pool<Vec<Tick>> {
        Pool::new_with_tick_data_provider(
            TOKEN0.clone().into(),
            TOKEN1.clone().into(),
            uint!(1_000_000_U24),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            liquidity,
            ticks,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn issue74_full_fee_traverses_empty_ranges_both_directions() {
        for zero_for_one in [true, false] {
            let direction = if zero_for_one { -1 } else { 1 };
            let limit = get_sqrt_ratio_at_tick((direction * 5_i32).to_i24()).unwrap();
            let pool = full_fee_test_pool(
                0,
                vec![
                    Tick {
                        index: -100,
                        liquidity_net: 0,
                        liquidity_gross: 1,
                    },
                    Tick {
                        index: 100,
                        liquidity_net: 0,
                        liquidity_gross: 1,
                    },
                ],
            );
            let state = pool
                .swap(zero_for_one, I256::from_raw(U256::from(100)), Some(limit))
                .await
                .unwrap();
            assert_eq!(state.sqrt_price_x96, limit);
            assert_eq!(state.tick_current, direction * 5);
            assert_eq!(
                state.amount_specified_remaining,
                I256::from_raw(U256::from(100))
            );
            assert_eq!(state.amount_calculated, I256::ZERO);

            let ticks = vec![
                Tick {
                    index: -10,
                    liquidity_net: 1000,
                    liquidity_gross: 1000,
                },
                Tick {
                    index: 10,
                    liquidity_net: -1000,
                    liquidity_gross: 1000,
                },
            ];
            // Outside the funded range, crossing its nearest boundary restores liquidity.
            let mut pool = full_fee_test_pool(0, ticks);
            pool.sqrt_price_x96 = get_sqrt_ratio_at_tick((direction * 20_i32).to_i24()).unwrap();
            pool.tick_current = direction * 20;
            let state = pool
                .swap(
                    !zero_for_one,
                    I256::from_raw(U256::from(100)),
                    Some(*SQRT_PRICE_1_1),
                )
                .await
                .unwrap();
            assert_eq!(
                state.sqrt_price_x96,
                get_sqrt_ratio_at_tick((direction * 10_i32).to_i24()).unwrap()
            );
            assert_eq!(state.tick_current, if zero_for_one { -10 } else { 9 });
            assert_eq!(state.liquidity, 1000);
            assert_eq!(state.amount_specified_remaining, I256::ZERO);
            assert_eq!(state.amount_calculated, I256::ZERO);
        }
    }

    #[tokio::test]
    async fn issue74_full_fee_crosses_same_price_boundary_and_rejects_exact_output() {
        let ticks = vec![
            Tick {
                index: -10,
                liquidity_net: 1000,
                liquidity_gross: 1000,
            },
            Tick {
                index: 0,
                liquidity_net: -1000,
                liquidity_gross: 1000,
            },
        ];
        let pool = full_fee_test_pool(1000, ticks);
        let state = pool
            .swap(true, I256::from_raw(U256::from(100)), None)
            .await
            .unwrap();
        assert_eq!(state.sqrt_price_x96, *SQRT_PRICE_1_1);
        assert_eq!(state.tick_current, -1);
        assert_eq!(state.liquidity, 2000);
        assert_eq!(state.amount_specified_remaining, I256::ZERO);
        assert!(matches!(
            pool.swap(true, -I256::ONE, None).await,
            Err(Error::InvalidFeeForExactOutput)
        ));
    }

    #[tokio::test]
    #[should_panic(expected = "RATIO_CURRENT")]
    async fn issue74_full_fee_validates_price_limit() {
        let pool = full_fee_test_pool(0, TICK_LIST.clone());
        let _ = pool.swap(true, I256::ONE, Some(*SQRT_PRICE_1_1)).await;
    }

    #[test]
    fn issue74_fee_validation_and_identity() {
        let pool = full_fee_test_pool(0, TICK_LIST.clone());
        assert!(
            pool.clone()
                .with_fees(uint!(999999_U24), U24::ZERO)
                .is_err()
        );
        for packed in [1001, 1001 << 12, 0xffffff] {
            assert!(pool.clone().with_fees(pool.fee, U24::from(packed)).is_err());
        }
        let dynamic = Pool::new_with_tick_data_provider(
            TOKEN0.clone().into(),
            TOKEN1.clone().into(),
            DYANMIC_FEE_FLAG,
            10,
            Address::with_last_byte(1),
            *SQRT_PRICE_1_1,
            0,
            TICK_LIST.clone(),
        )
        .unwrap();
        let resolved = dynamic
            .clone()
            .with_fees(uint!(3000_U24), uint!(1000_U24))
            .unwrap();
        assert_eq!(resolved.pool_key, dynamic.pool_key);
        assert_eq!(resolved.pool_id, dynamic.pool_id);
        assert_eq!(resolved.fee, DYANMIC_FEE_FLAG);
        assert_ne!(resolved, dynamic);
        assert_eq!(resolved.clone(), resolved);
        assert!(dynamic.with_fees(uint!(1000001_U24), U24::ZERO).is_err());
    }

    #[tokio::test]
    async fn issue74_full_fee_quotes_preserve_identity_and_state() {
        let pool = full_fee_test_pool(ONE_ETHER, TICK_LIST.clone())
            .with_fees(uint!(1_000_000_U24), U24::from(1000 + (500 << 12)))
            .unwrap();
        for token in [TOKEN0.clone(), TOKEN1.clone()] {
            let (output, quoted) = pool
                .get_output_amount(&CurrencyAmount::from_raw_amount(token, 100).unwrap(), None)
                .await
                .unwrap();
            assert_eq!(output.quotient(), BigInt::from(0));
            let mut expected = pool.clone();
            // The zero-for-one word boundary at tick 0 is crossed at zero cost.
            expected.tick_current = if output.currency == pool.currency1 {
                -1
            } else {
                0
            };
            assert_eq!(quoted, expected);
            assert_eq!(quoted.pool_id, pool.pool_id);
            assert_eq!(quoted.pool_key, pool.pool_key);
        }
    }

    #[test]
    fn issue74_full_fee_pool_is_valid() {
        Pool::new(
            TOKEN0.clone().into(),
            TOKEN1.clone().into(),
            uint!(1_000_000_U24),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap();
    }

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
                uint!(1_000_001_U24),
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
                encode_sqrt_ratio_x96(
                    BigInt::from_u128(101e6 as u128).unwrap(),
                    BigInt::from_u128(100e18 as u128).unwrap()
                ),
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
                encode_sqrt_ratio_x96(
                    BigInt::from_u128(101e6 as u128).unwrap(),
                    BigInt::from_u128(100e18 as u128).unwrap()
                ),
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
                encode_sqrt_ratio_x96(
                    BigInt::from_u128(101e6 as u128).unwrap(),
                    BigInt::from_u128(100e18 as u128).unwrap()
                ),
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
                encode_sqrt_ratio_x96(
                    BigInt::from_u128(101e6 as u128).unwrap(),
                    BigInt::from_u128(100e18 as u128).unwrap()
                ),
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
