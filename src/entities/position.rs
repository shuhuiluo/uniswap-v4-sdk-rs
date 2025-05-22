use crate::prelude::{tick_to_price, Error, Pool, *};
use alloc::vec;
use alloy_primitives::{
    aliases::{I24, U48},
    keccak256, uint, U160, U256,
};
use alloy_sol_types::SolValue;
use num_traits::ToPrimitive;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

/// Represents a position on a Uniswap V4 Pool
#[derive(Clone, Debug)]
pub struct Position<TP = NoTickDataProvider>
where
    TP: TickDataProvider,
{
    pub pool: Pool<TP>,
    pub tick_lower: TP::Index,
    pub tick_upper: TP::Index,
    pub liquidity: u128,
    _token0_amount: Option<CurrencyAmount<Currency>>,
    _token1_amount: Option<CurrencyAmount<Currency>>,
    _mint_amounts: Option<MintAmounts>,
}

impl<TP: TickDataProvider> Position<TP> {
    /// Constructs a position for a given pool with the given liquidity
    ///
    /// ## Arguments
    ///
    /// * `pool`: For which pool the liquidity is assigned
    /// * `liquidity`: The amount of liquidity that is in the position
    /// * `tick_lower`: The lower tick of the position
    /// * `tick_upper`: The upper tick of the position
    #[inline]
    pub fn new(
        pool: Pool<TP>,
        liquidity: u128,
        tick_lower: TP::Index,
        tick_upper: TP::Index,
    ) -> Self {
        assert!(tick_lower < tick_upper, "TICK_ORDER");
        assert!(
            tick_lower >= TP::Index::from_i24(MIN_TICK)
                && (tick_lower % pool.tick_spacing).is_zero(),
            "TICK_LOWER"
        );
        assert!(
            tick_upper <= TP::Index::from_i24(MAX_TICK)
                && (tick_upper % pool.tick_spacing).is_zero(),
            "TICK_UPPER"
        );
        Self {
            pool,
            liquidity,
            tick_lower,
            tick_upper,
            _token0_amount: None,
            _token1_amount: None,
            _mint_amounts: None,
        }
    }

    /// Returns the price of token0 at the lower tick
    #[inline]
    pub fn token0_price_lower(&self) -> Result<Price<Currency, Currency>, Error> {
        tick_to_price(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.tick_lower.to_i24(),
        )
    }

    /// Returns the price of token0 at the upper tick
    #[inline]
    pub fn token0_price_upper(&self) -> Result<Price<Currency, Currency>, Error> {
        tick_to_price(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.tick_upper.to_i24(),
        )
    }

    /// Returns the amount of token0 that this position's liquidity could be burned for at the
    /// current pool price
    #[inline]
    pub fn amount0(&self) -> Result<CurrencyAmount<Currency>, Error> {
        if self.pool.tick_current < self.tick_lower {
            CurrencyAmount::from_raw_amount(
                self.pool.currency0.clone(),
                get_amount_0_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    false,
                )?
                .to_big_int(),
            )
        } else if self.pool.tick_current < self.tick_upper {
            CurrencyAmount::from_raw_amount(
                self.pool.currency0.clone(),
                get_amount_0_delta(
                    self.pool.sqrt_price_x96,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    false,
                )?
                .to_big_int(),
            )
        } else {
            CurrencyAmount::from_raw_amount(self.pool.currency0.clone(), BigInt::ZERO)
        }
        .map_err(Error::Core)
    }

    /// Returns the amount of token0 that this position's liquidity could be burned for at the
    /// current pool price
    #[inline]
    pub fn amount0_cached(&mut self) -> Result<CurrencyAmount<Currency>, Error> {
        if let Some(amount) = &self._token0_amount {
            return Ok(amount.clone());
        }
        let amount = self.amount0()?;
        self._token0_amount = Some(amount.clone());
        Ok(amount)
    }

    /// Returns the amount of token1 that this position's liquidity could be burned for at the
    /// current pool price
    #[inline]
    pub fn amount1(&self) -> Result<CurrencyAmount<Currency>, Error> {
        if self.pool.tick_current < self.tick_lower {
            CurrencyAmount::from_raw_amount(self.pool.currency1.clone(), BigInt::ZERO)
        } else if self.pool.tick_current < self.tick_upper {
            CurrencyAmount::from_raw_amount(
                self.pool.currency1.clone(),
                get_amount_1_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    self.pool.sqrt_price_x96,
                    self.liquidity,
                    false,
                )?
                .to_big_int(),
            )
        } else {
            CurrencyAmount::from_raw_amount(
                self.pool.currency1.clone(),
                get_amount_1_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    false,
                )?
                .to_big_int(),
            )
        }
        .map_err(Error::Core)
    }

    /// Returns the amount of token1 that this position's liquidity could be burned for at the
    /// current pool price
    #[inline]
    pub fn amount1_cached(&mut self) -> Result<CurrencyAmount<Currency>, Error> {
        if let Some(amount) = &self._token1_amount {
            return Ok(amount.clone());
        }
        let amount = self.amount1()?;
        self._token1_amount = Some(amount.clone());
        Ok(amount)
    }

    /// Returns the lower and upper sqrt ratios if the price 'slips' up to slippage tolerance
    /// percentage
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The amount by which the price can 'slip' before the transaction will
    ///   revert
    ///
    /// ## Returns
    ///
    /// (sqrt_ratio_x96_lower, sqrt_ratio_x96_upper)
    fn ratios_after_slippage(&self, slippage_tolerance: &Percent) -> (U160, U160) {
        let one = Percent::new(1, 1);
        let token0_price = self.pool.token0_price().as_fraction();
        let price_lower = (one.clone() - slippage_tolerance).as_fraction() * &token0_price;
        let price_upper = token0_price * ((one + slippage_tolerance).as_fraction());

        let mut sqrt_ratio_x96_lower =
            encode_sqrt_ratio_x96(price_lower.numerator, price_lower.denominator);
        if sqrt_ratio_x96_lower <= MIN_SQRT_RATIO {
            sqrt_ratio_x96_lower = MIN_SQRT_RATIO + uint!(1_U160);
        }

        let sqrt_ratio_x96_upper = if price_upper
            >= Fraction::new(MAX_SQRT_RATIO.to_big_int().pow(2), Q192.to_big_int())
        {
            MAX_SQRT_RATIO - uint!(1_U160)
        } else {
            encode_sqrt_ratio_x96(price_upper.numerator, price_upper.denominator)
        };

        (sqrt_ratio_x96_lower, sqrt_ratio_x96_upper)
    }

    //   /**
    //    * Calculates the adjusted amounts and adjusted liquidity for the position given the
    //      slippage tolerance.
    //    * The function:
    //    * 1. Adjusts the liquidity based on the slippage tolerance
    //    * 2. Creates a new position with the adjusted liquidity
    //    * 3. Calculates the mint amounts for the adjusted position
    //    *
    //    * @param slippageTolerance The maximum acceptable slippage as a percentage
    //    * @returns An object containing:
    //    * - amount0: The adjusted amount of token0
    //    * - amount1: The adjusted amount of token1
    //    * - liquidity: The adjusted liquidity value
    //    */
    //
    //   public maxAmountsAndLiquidityWithSlippage(
    //     slippageTolerance: Percent
    //   ): Readonly<{ amount0: JSBI; amount1: JSBI; liquidity: JSBI }> {
    //     const adjustedLiquidity = this.getAdjustedLiquidityForSlippage(slippageTolerance)
    //     const position = new Position({
    //       pool: this.pool,
    //       liquidity: adjustedLiquidity,
    //       tickLower: this.tickLower,
    //       tickUpper: this.tickUpper,
    //     })
    //     const { amount0, amount1 } = position.mintAmountsWithSlippage(slippageTolerance)
    //     return { amount0, amount1, liquidity: adjustedLiquidity }
    //   }
    //
    //   /**
    //    * Calculates the adjusted liquidity for a position given a slippage tolerance.
    //    * The function:
    //    * 1. Gets the upper and lower price bounds after slippage
    //    * 2. Creates two counterfactual pools at these price bounds
    //    * 3. Calculates the liquidity that would be obtained at each price bound
    //    * 4. Returns the minimum liquidity to ensure the position can be created at either price
    //      bound
    //    *
    //    * @param slippageTolerance The maximum acceptable slippage as a percentage
    //    * @returns The adjusted liquidity value that ensures the position can be created even with
    //      slippage
    //    */
    //   private getAdjustedLiquidityForSlippage(slippageTolerance: Percent): JSBI {
    //     const { sqrtRatioX96Upper, sqrtRatioX96Lower } =
    // this.ratiosAfterSlippage(slippageTolerance)     // construct counterfactual pools from
    // the lower bounded price and the upper bounded price     const poolLower = new Pool(
    //       this.pool.token0,
    //       this.pool.token1,
    //       this.pool.fee,
    //       this.pool.tickSpacing,
    //       this.pool.hooks,
    //       sqrtRatioX96Lower,
    //       0 /* liquidity doesn't matter */,
    //       TickMath.getTickAtSqrtRatio(sqrtRatioX96Lower)
    //     )
    //     const poolUpper = new Pool(
    //       this.pool.token0,
    //       this.pool.token1,
    //       this.pool.fee,
    //       this.pool.tickSpacing,
    //       this.pool.hooks,
    //       sqrtRatioX96Upper,
    //       0 /* liquidity doesn't matter */,
    //       TickMath.getTickAtSqrtRatio(sqrtRatioX96Upper)
    //     )
    //
    //     const { amount0, amount1 } = this.mintAmounts
    //     const positionUpper = Position.fromAmounts({
    //       pool: poolUpper,
    //       tickLower: this.tickLower,
    //       tickUpper: this.tickUpper,
    //       amount0,
    //       amount1,
    //       useFullPrecision: true,
    //     })
    //     const liquidityUpper = positionUpper.liquidity
    //
    //     const positionLower = Position.fromAmounts({
    //       pool: poolLower,
    //       tickLower: this.tickLower,
    //       tickUpper: this.tickUpper,
    //       amount0,
    //       amount1,
    //       useFullPrecision: true,
    //     })
    //     const liquidityLower = positionLower.liquidity
    //
    //     if (JSBI.lessThanOrEqual(liquidityUpper, liquidityLower)) {
    //       return liquidityUpper
    //     } else {
    //       return liquidityLower
    //     }
    //   }
    //
    //   /**
    //    * Returns the maximum amount of token0 and token1 that must be sent in order to safely
    //      mint the amount of liquidity held by the position
    //    * with the given slippage tolerance
    //    * @param slippageTolerance Tolerance of unfavorable slippage from the current price
    //    * @returns The amounts, with slippage
    //    * @dev In v4, minting and increasing is protected by maximum amounts of token0 and token1.
    //    */
    //   public getMintAmountsWithSlippage(slippageTolerance: Percent): Readonly<{ amount0: JSBI;
    // amount1: JSBI }> {     return this.mintAmountsWithSlippage(slippageTolerance)
    //   }
    //
    //   /**
    //    * Internal helper method that calculates the maximum amounts of token0 and token1 needed
    //      for minting
    //    * with slippage protection. This is used by the public getMintAmountsWithSlippage method.
    //    * @param slippageTolerance Tolerance of unfavorable slippage from the current price
    //    * @returns The amounts, with slippage
    //    * @dev In v4, minting and increasing is protected by maximum amounts of token0 and token1.
    //    */
    //   private mintAmountsWithSlippage(slippageTolerance: Percent): Readonly<{ amount0: JSBI;
    // amount1: JSBI }> {

    /// Returns the maximum amounts that must be sent in order to safely mint the amount of
    /// liquidity held by the position
    ///
    /// ## Note
    ///
    /// In v4, minting and increasing is protected by maximum amounts of token0 and token1.
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: Tolerance of unfavorable slippage from the current price
    ///
    /// ## Returns
    ///
    /// The amounts, with slippage
    #[inline]
    fn mint_amounts_with_slippage(
        &mut self,
        slippage_tolerance: &Percent,
    ) -> Result<MintAmounts, Error> {
        // get lower/upper prices
        // these represent the lowest and highest prices that the pool is allowed to "slip" to
        let (sqrt_ratio_x96_lower, sqrt_ratio_x96_upper) =
            self.ratios_after_slippage(slippage_tolerance);

        // construct counterfactual pools from the lower bounded price and the upper bounded price
        let pool_lower = Pool::new(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.pool.fee,
            self.pool.tick_spacing.to_i24().as_i32(),
            self.pool.hooks,
            sqrt_ratio_x96_lower,
            0, // liquidity doesn't matter
        )?;
        let pool_upper = Pool::new(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.pool.fee,
            self.pool.tick_spacing.to_i24().as_i32(),
            self.pool.hooks,
            sqrt_ratio_x96_upper,
            0, // liquidity doesn't matter
        )?;

        // because the router is imprecise, we need to calculate the position (assuming no slippage)
        // to get the estimated actual liquidity
        let MintAmounts { amount0, amount1 } = self.mint_amounts_cached()?;
        let position_without_slippage = Position::from_amounts(
            Pool::new(
                self.pool.currency0.clone(),
                self.pool.currency1.clone(),
                self.pool.fee,
                self.pool.tick_spacing.to_i24().as_i32(),
                self.pool.hooks,
                self.pool.sqrt_price_x96,
                self.pool.liquidity,
            )?,
            self.tick_lower.try_into().unwrap(),
            self.tick_upper.try_into().unwrap(),
            amount0,
            amount1,
            false,
        )?;

        // Note: Slippage derivation in v4 is different from v3.
        // When creating a position (minting) or adding to a position (increasing) slippage is
        // bounded by the MAXIMUM amount in in token0 and token1.
        // The largest amount of token1 will happen when the price slips up, so we use the poolUpper
        // to get amount1.
        // The largest amount of token0 will happen when the price slips
        // down, so we use the poolLower to get amount0.
        // Ie...We want the larger amounts, which occurs at the upper price for amount1...
        let amount1 = Position::new(
            pool_upper,
            position_without_slippage.liquidity,
            self.tick_lower.try_into().unwrap(),
            self.tick_upper.try_into().unwrap(),
        )
        .mint_amounts()?
        .amount1;
        // ...and the lower for amount0
        let amount0 = Position::new(
            pool_lower,
            position_without_slippage.liquidity,
            self.tick_lower.try_into().unwrap(),
            self.tick_upper.try_into().unwrap(),
        )
        .mint_amounts()?
        .amount0;

        Ok(MintAmounts { amount0, amount1 })
    }

    /// Returns the minimum amounts that should be requested in order to safely burn the amount of
    /// liquidity held by the position with the given slippage tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: tolerance of unfavorable slippage from the current price
    ///
    /// ## Returns
    ///
    /// The amounts, with slippage
    #[inline]
    pub fn burn_amounts_with_slippage(
        &self,
        slippage_tolerance: &Percent,
    ) -> Result<(U256, U256), Error> {
        // get lower/upper prices
        let (sqrt_ratio_x96_lower, sqrt_ratio_x96_upper) =
            self.ratios_after_slippage(slippage_tolerance);

        // construct counterfactual pools
        let pool_lower = Pool::new(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.pool.fee,
            self.pool.tick_spacing.to_i24().as_i32(),
            self.pool.hooks,
            sqrt_ratio_x96_lower,
            0, // liquidity doesn't matter
        )?;
        let pool_upper = Pool::new(
            self.pool.currency0.clone(),
            self.pool.currency1.clone(),
            self.pool.fee,
            self.pool.tick_spacing.to_i24().as_i32(),
            self.pool.hooks,
            sqrt_ratio_x96_upper,
            0, // liquidity doesn't matter
        )?;

        // we want the smaller amounts...
        // ...which occurs at the upper price for amount0...
        let amount0 = Position::new(
            pool_upper,
            self.liquidity,
            self.tick_lower.try_into().unwrap(),
            self.tick_upper.try_into().unwrap(),
        )
        .amount0()?
        .quotient();
        // ...and the lower for amount1
        let amount1 = Position::new(
            pool_lower,
            self.liquidity,
            self.tick_lower.try_into().unwrap(),
            self.tick_upper.try_into().unwrap(),
        )
        .amount1()?
        .quotient();

        Ok((U256::from_big_int(amount0), U256::from_big_int(amount1)))
    }

    /// Returns the minimum amounts that must be sent in order to mint the amount of liquidity held
    /// by the position at the current price for the pool
    #[inline]
    pub fn mint_amounts(&self) -> Result<MintAmounts, Error> {
        Ok(if self.pool.tick_current < self.tick_lower {
            MintAmounts {
                amount0: get_amount_0_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    true,
                )?,
                amount1: U256::ZERO,
            }
        } else if self.pool.tick_current < self.tick_upper {
            MintAmounts {
                amount0: get_amount_0_delta(
                    self.pool.sqrt_price_x96,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    true,
                )?,
                amount1: get_amount_1_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    self.pool.sqrt_price_x96,
                    self.liquidity,
                    true,
                )?,
            }
        } else {
            MintAmounts {
                amount0: U256::ZERO,
                amount1: get_amount_1_delta(
                    get_sqrt_ratio_at_tick(self.tick_lower.to_i24())?,
                    get_sqrt_ratio_at_tick(self.tick_upper.to_i24())?,
                    self.liquidity,
                    true,
                )?,
            }
        })
    }

    /// Returns the minimum amounts that must be sent in order to mint the amount of liquidity held
    /// by the position at the current price for the pool
    #[inline]
    pub fn mint_amounts_cached(&mut self) -> Result<MintAmounts, Error> {
        if let Some(amounts) = &self._mint_amounts {
            return Ok(*amounts);
        }
        let amounts = self.mint_amounts()?;
        self._mint_amounts = Some(amounts);
        Ok(amounts)
    }

    /// Returns the [`AllowanceTransferPermitBatch`] for adding liquidity to a position
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The amount by which the price can 'slip' before the transaction will
    ///   revert
    /// * `spender`: The spender of the permit (should usually be the [`PositionManager`])
    /// * `nonce`: A valid permit2 nonce
    /// * `deadline`: The deadline for the permit
    #[inline]
    pub fn permit_batch_data(
        &mut self,
        slippage_tolerance: &Percent,
        spender: Address,
        nonce: U256,
        deadline: U256,
    ) -> Result<AllowanceTransferPermitBatch, Error> {
        let MintAmounts { amount0, amount1 } =
            self.mint_amounts_with_slippage(slippage_tolerance)?;
        Ok(AllowanceTransferPermitBatch {
            details: vec![
                IAllowanceTransfer::PermitDetails {
                    token: self.pool.currency0.wrapped().address(),
                    amount: U160::from(amount0),
                    expiration: U48::from(deadline),
                    nonce: U48::from(nonce),
                },
                IAllowanceTransfer::PermitDetails {
                    token: self.pool.currency1.wrapped().address(),
                    amount: U160::from(amount1),
                    expiration: U48::from(deadline),
                    nonce: U48::from(nonce),
                },
            ],
            spender,
            sigDeadline: deadline,
        })
    }

    /// Computes the maximum amount of liquidity received for a given amount of token0, token1,
    /// and the prices at the tick boundaries.
    ///
    /// ## Arguments
    ///
    /// * `pool`: The pool for which the position should be created
    /// * `tick_lower`: The lower tick of the position
    /// * `tick_upper`: The upper tick of the position
    /// * `amount0`: token0 amount
    /// * `amount1`: token1 amount
    /// * `use_full_precision`: If false, liquidity will be maximized according to what the router
    ///   can calculate, not what core can theoretically support
    ///
    /// ## Returns
    ///
    /// The position with the maximum amount of liquidity received
    #[inline]
    pub fn from_amounts(
        pool: Pool<TP>,
        tick_lower: TP::Index,
        tick_upper: TP::Index,
        amount0: U256,
        amount1: U256,
        use_full_precision: bool,
    ) -> Result<Self, Error> {
        let sqrt_ratio_a_x96 = get_sqrt_ratio_at_tick(tick_lower.to_i24())?;
        let sqrt_ratio_b_x96 = get_sqrt_ratio_at_tick(tick_upper.to_i24())?;
        let liquidity = max_liquidity_for_amounts(
            pool.sqrt_price_x96,
            sqrt_ratio_a_x96,
            sqrt_ratio_b_x96,
            amount0,
            amount1,
            use_full_precision,
        );
        Ok(Self::new(
            pool,
            liquidity.to_u128().unwrap(),
            tick_lower,
            tick_upper,
        ))
    }

    /// Computes a position with the maximum amount of liquidity received for a given amount of
    /// token0, assuming an unlimited amount of token1
    ///
    /// ## Arguments
    ///
    /// * `pool`: The pool for which the position is created
    /// * `tick_lower`: The lower tick
    /// * `tick_upper`: The upper tick
    /// * `amount0`: The desired amount of token0
    /// * `use_full_precision`: If true, liquidity will be maximized according to what the router
    ///   can calculate, not what core can theoretically support
    #[inline]
    pub fn from_amount0(
        pool: Pool<TP>,
        tick_lower: TP::Index,
        tick_upper: TP::Index,
        amount0: U256,
        use_full_precision: bool,
    ) -> Result<Self, Error> {
        Self::from_amounts(
            pool,
            tick_lower,
            tick_upper,
            amount0,
            U256::MAX,
            use_full_precision,
        )
    }

    /// Computes a position with the maximum amount of liquidity received for a given amount of
    /// token1, assuming an unlimited amount of token0
    ///
    /// ## Arguments
    ///
    /// * `pool`: The pool for which the position is created
    /// * `tick_lower`: The lower tick
    /// * `tick_upper`: The upper tick
    /// * `amount1`: The desired amount of token1
    #[inline]
    pub fn from_amount1(
        pool: Pool<TP>,
        tick_lower: TP::Index,
        tick_upper: TP::Index,
        amount1: U256,
    ) -> Result<Self, Error> {
        // this function always uses full precision
        Self::from_amounts(pool, tick_lower, tick_upper, U256::MAX, amount1, true)
    }
}

/// Computes the position key for a given position
#[inline]
#[must_use]
pub fn calculate_position_key(
    owner: Address,
    tick_lower: I24,
    tick_upper: I24,
    salt: B256,
) -> B256 {
    keccak256((owner, tick_lower, tick_upper, salt).abi_encode_packed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::token;

    static CURRENCY0: Lazy<Token> = Lazy::new(|| {
        token!(
            1,
            "0x0000000000000000000000000000000000000001",
            18,
            "t0",
            "currency0"
        )
    });
    static CURRENCY1: Lazy<Token> = Lazy::new(|| {
        token!(
            1,
            "0x0000000000000000000000000000000000000002",
            18,
            "t1",
            "currency1"
        )
    });
    const FEE: FeeAmount = FeeAmount::MEDIUM;
    static POOL_0_1: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY0.clone().into(),
            CURRENCY1.clone().into(),
            FEE.into(),
            FEE.tick_spacing().as_i32(),
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            0,
        )
        .unwrap()
    });
    static SLIPPAGE_TOLERANCE: Lazy<Percent> = Lazy::new(|| Percent::new(1, 100));

    mod bounds_maximum_amounts {
        use super::*;

        #[tokio::test]
        async fn in_range() {
            let mut position = Position::new(
                POOL_0_1.clone(),
                100_000_000_000_000_000_000u128, // 100e18
                -180,
                180,
            );

            let MintAmounts {
                amount0: initialized_amount0,
                amount1: initialized_amount1,
            } = position.mint_amounts().unwrap();

            let MintAmounts {
                amount0: adjusted_amount0,
                amount1: adjusted_amount1,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE)
                .unwrap();

            assert!(
                adjusted_amount0 <= initialized_amount0,
                "adjusted amount0 should be <= initialized amount0"
            );
            assert!(
                adjusted_amount1 <= initialized_amount1,
                "adjusted amount1 should be <= initialized amount1"
            );
            assert!(
                adjusted_amount0 == initialized_amount0 || adjusted_amount1 == initialized_amount1,
                "at least one of the adjusted amounts should equal
    initialized amount"
            );
        }

        #[tokio::test]
        async fn below_range() {
            let position = Position::new(
                pool_0_1.clone(),
                100_000_000_000_000_000_000u128, // 100e18
                -180,
                -60,
            );

            let mint_amounts = position.mint_amounts().unwrap();
            let initialized_amount0 = mint_amounts.amount0;
            let initialized_amount1 = mint_amounts.amount1;

            let slippage_amounts = position
                .mint_amounts_with_slippage(&slippage_tolerance)
                .unwrap();
            let adjusted_amount0 = slippage_amounts.amount0;
            let adjusted_amount1 = slippage_amounts.amount1;

            assert!(
                adjusted_amount0 <= initialized_amount0,
                "adjusted amount0 should be <= initialized amount0"
            );
            assert!(
                adjusted_amount1 <= initialized_amount1,
                "adjusted amount1 should be <= initialized amount1"
            );
            assert!(
                adjusted_amount0 == initialized_amount0 || adjusted_amount1 == initialized_amount1,
                "at least one of the adjusted amounts should equal
    initialized amount"
            );
        }

        #[tokio::test]
        async fn above_range() {
            let position = Position::new(
                pool_0_1,
                100_000_000_000_000_000_000u128, // 100e18
                60,
                180,
            );

            let mint_amounts = position.mint_amounts().unwrap();
            let initialized_amount0 = mint_amounts.amount0;
            let initialized_amount1 = mint_amounts.amount1;

            let slippage_amounts = position
                .mint_amounts_with_slippage(&slippage_tolerance)
                .unwrap();
            let adjusted_amount0 = slippage_amounts.amount0;
            let adjusted_amount1 = slippage_amounts.amount1;

            assert!(
                adjusted_amount0 <= initialized_amount0,
                "adjusted amount0 should be <= initialized amount0"
            );
            assert!(
                adjusted_amount1 <= initialized_amount1,
                "adjusted amount1 should be <= initialized amount1"
            );
            assert!(
                adjusted_amount0 == initialized_amount0 || adjusted_amount1 == initialized_amount1,
                "at least one of the adjusted amounts should equal
    initialized amount"
            );
        }
    }

    // #[tokio::test]
    // async fn test_mint_amounts() {
    //     // Setup tokens similar to the TypeScript tests
    //     let usdc = Token::new(
    //         1,
    //         "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
    //             .parse()
    //             .unwrap(),
    //         6,
    //         "USDC",
    //         "USD Coin",
    //     );
    //     let dai = Token::new(
    //         1,
    //         "0x6B175474E89094C44Da98b954EedeAC495271d0F"
    //             .parse()
    //             .unwrap(),
    //         18,
    //         "DAI",
    //         "DAI Stablecoin",
    //     );
    //
    //     let pool_sqrt_ratio_start = encode_sqrt_ratio_x96(
    //         U256::from_dec_str("100000000").unwrap(), // 100e6
    //         U256::from_dec_str("100000000000000000000").unwrap(), // 100e18
    //     );
    //
    //     let pool_tick_current = get_tick_at_sqrt_ratio(pool_sqrt_ratio_start).unwrap();
    //     let tick_spacing = 10; // TICK_SPACING_TEN
    //
    //     let dai_usdc_pool = Pool::new(
    //         dai.clone(),
    //         usdc.clone(),
    //         500,
    //         tick_spacing,
    //         ADDRESS_ZERO,
    //         pool_sqrt_ratio_start,
    //         0,
    //         pool_tick_current.into(),
    //         vec![],
    //     )
    //     .unwrap();
    //
    //     // 0 slippage tolerance
    //     let slippage_tolerance = Percent::new(0, 1);
    //
    //     // Test positions below
    //     {
    //         let tick_lower =
    //             nearest_usable_tick(pool_tick_current.into(), tick_spacing.into()) +
    // tick_spacing;         let tick_upper = nearest_usable_tick(pool_tick_current.into(),
    // tick_spacing.into())
    //             + tick_spacing * 2;
    //
    //         let amount0 = U256::from_dec_str("49949961958869841738198").unwrap();
    //         let amount1 = U256::ZERO;
    //
    //         let liquidity = max_liquidity_for_amounts(
    //             dai_usdc_pool.sqrt_price_x96,
    //             get_sqrt_ratio_at_tick(tick_lower).unwrap(),
    //             get_sqrt_ratio_at_tick(tick_upper).unwrap(),
    //             amount0,
    //             amount1,
    //             true,
    //         );
    //
    //         let position = Position::new(
    //             dai_usdc_pool.clone(),
    //             liquidity.to_u128().unwrap(),
    //             tick_lower,
    //             tick_upper,
    //         );
    //
    //         let mint_amounts = position
    //             .mint_amounts_with_slippage(&slippage_tolerance)
    //             .unwrap();
    //
    //         assert_eq!(
    //             mint_amounts.amount0.to_string(),
    //             "49949961958869841738198",
    //             "amount0 should match expected value"
    //         );
    //         assert_eq!(
    //             mint_amounts.amount1.to_string(),
    //             "0",
    //             "amount1 should match expected value"
    //         );
    //     }
    //
    //     // Test positions above
    //     {
    //         let tick_lower = nearest_usable_tick(pool_tick_current.into(), tick_spacing.into())
    //             - tick_spacing * 2;
    //         let tick_upper =
    //             nearest_usable_tick(pool_tick_current.into(), tick_spacing.into()) -
    // tick_spacing;
    //
    //         let amount0 = U256::ZERO;
    //         let amount1 = U256::from_dec_str("49970077053").unwrap();
    //
    //         let liquidity = max_liquidity_for_amounts(
    //             dai_usdc_pool.sqrt_price_x96,
    //             get_sqrt_ratio_at_tick(tick_lower).unwrap(),
    //             get_sqrt_ratio_at_tick(tick_upper).unwrap(),
    //             amount0,
    //             amount1,
    //             true,
    //         );
    //
    //         let position = Position::new(
    //             dai_usdc_pool.clone(),
    //             liquidity.to_u128().unwrap(),
    //             tick_lower,
    //             tick_upper,
    //         );
    //
    //         let mint_amounts = position
    //             .mint_amounts_with_slippage(&slippage_tolerance)
    //             .unwrap();
    //
    //         assert_eq!(
    //             mint_amounts.amount0.to_string(),
    //             "0",
    //             "amount0 should match expected value"
    //         );
    //         assert_eq!(
    //             mint_amounts.amount1.to_string(),
    //             "49970077053",
    //             "amount1 should match expected value"
    //         );
    //     }
    //
    //     // Test positions within
    //     {
    //         let tick_lower = nearest_usable_tick(pool_tick_current.into(), tick_spacing.into())
    //             - tick_spacing * 2;
    //         let tick_upper = nearest_usable_tick(pool_tick_current.into(), tick_spacing.into())
    //             + tick_spacing * 2;
    //
    //         let amount0 = U256::from_dec_str("120054069145287995740584").unwrap();
    //         let amount1 = U256::from_dec_str("79831926243").unwrap();
    //
    //         let liquidity = max_liquidity_for_amounts(
    //             dai_usdc_pool.sqrt_price_x96,
    //             get_sqrt_ratio_at_tick(tick_lower).unwrap(),
    //             get_sqrt_ratio_at_tick(tick_upper).unwrap(),
    //             amount0,
    //             amount1,
    //             true,
    //         );
    //
    //         let position = Position::new(
    //             dai_usdc_pool.clone(),
    //             liquidity.to_u128().unwrap(),
    //             tick_lower,
    //             tick_upper,
    //         );
    //
    //         let mint_amounts = position
    //             .mint_amounts_with_slippage(&slippage_tolerance)
    //             .unwrap();
    //
    //         assert_eq!(
    //             mint_amounts.amount0.to_string(),
    //             "120054069145287995740584",
    //             "amount0 should match expected value"
    //         );
    //         assert_eq!(
    //             mint_amounts.amount1.to_string(),
    //             "79831926243",
    //             "amount1 should match expected value"
    //         );
    //     }
    // }
}
