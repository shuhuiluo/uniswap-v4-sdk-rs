use crate::prelude::{amount_with_path_currency, Error, Pool, Route};
use alloc::{boxed::Box, vec};
use alloy_primitives::map::HashSet;
use core::cmp::Ordering;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

/// Trades comparator, an extension of the input output comparator that also considers other
/// dimensions of the trade in ranking them
///
/// ## Arguments
///
/// * `a`: The first trade to compare
/// * `b`: The second trade to compare
#[inline]
pub fn trade_comparator<TInput, TOutput, TP>(
    a: &Trade<TInput, TOutput, TP>,
    b: &Trade<TInput, TOutput, TP>,
) -> Ordering
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    // must have same input and output token for comparison
    assert!(
        a.input_currency().equals(b.input_currency()),
        "INPUT_CURRENCY"
    );
    assert!(
        a.output_currency().equals(b.output_currency()),
        "OUTPUT_CURRENCY"
    );
    let a_input = a.input_amount().unwrap().as_fraction();
    let b_input = b.input_amount().unwrap().as_fraction();
    let a_output = a.output_amount().unwrap().as_fraction();
    let b_output = b.output_amount().unwrap().as_fraction();
    if a_output == b_output {
        if a_input == b_input {
            // consider the number of hops since each hop costs gas
            let a_hops = a
                .swaps
                .iter()
                .map(|s| s.route.pools.len() + 1)
                .sum::<usize>();
            let b_hops = b
                .swaps
                .iter()
                .map(|s| s.route.pools.len() + 1)
                .sum::<usize>();
            return a_hops.cmp(&b_hops);
        }
        // trade A requires less input than trade B, so A should come first
        if a_input < b_input {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else {
        // tradeA has less output than trade B, so should come second
        if a_output < b_output {
            Ordering::Greater
        } else {
            Ordering::Less
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BestTradeOptions {
    /// how many results to return
    pub max_num_results: Option<usize>,
    /// the maximum number of hops a trade should contain
    pub max_hops: Option<usize>,
}

/// Represents a swap through a route
#[derive(Clone, PartialEq, Debug)]
pub struct Swap<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    pub route: Route<TInput, TOutput, TP>,
    pub input_amount: CurrencyAmount<TInput>,
    pub output_amount: CurrencyAmount<TOutput>,
}

impl<TInput, TOutput, TP> Swap<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    /// Constructs a swap
    ///
    /// ## Arguments
    ///
    /// * `route`: The route of the swap
    /// * `input_amount`: The amount being passed in
    /// * `output_amount`: The amount returned by the swap
    #[inline]
    pub const fn new(
        route: Route<TInput, TOutput, TP>,
        input_amount: CurrencyAmount<TInput>,
        output_amount: CurrencyAmount<TOutput>,
    ) -> Self {
        Self {
            route,
            input_amount,
            output_amount,
        }
    }

    /// Returns the input currency of the swap
    #[inline]
    pub const fn input_currency(&self) -> &TInput {
        &self.input_amount.meta.currency
    }

    /// Returns the output currency of the swap
    #[inline]
    pub const fn output_currency(&self) -> &TOutput {
        &self.output_amount.meta.currency
    }
}

/// Represents a trade executed against a set of routes where some percentage of the input is split
/// across each route.
///
/// Each route has its own set of pools. Pools can not be re-used across routes.
///
/// Does not account for slippage, i.e., changes in price environment that can occur between the
/// time the trade is submitted and when it is executed.
#[derive(Clone, PartialEq, Debug)]
pub struct Trade<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    /// The swaps of the trade, i.e. which routes and how much is swapped in each that make up the
    /// trade.
    pub swaps: Vec<Swap<TInput, TOutput, TP>>,
    /// The type of the trade, either exact in or exact out.
    pub trade_type: TradeType,
    /// The cached result of the input amount computation
    _input_amount: Option<CurrencyAmount<TInput>>,
    /// The cached result of the output amount computation
    _output_amount: Option<CurrencyAmount<TOutput>>,
    /// The cached result of the computed execution price
    _execution_price: Option<Price<TInput, TOutput>>,
    /// The cached result of the price impact computation
    _price_impact: Option<Percent>,
}

impl<TInput, TOutput, TP> Trade<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: TickDataProvider,
{
    /// Construct a trade by passing in the pre-computed property values
    ///
    /// ## Arguments
    ///
    /// * `swaps`: The routes through which the trade occurs
    /// * `trade_type`: The type of trade, exact input or exact output
    #[inline]
    fn new(swaps: Vec<Swap<TInput, TOutput, TP>>, trade_type: TradeType) -> Result<Self, Error> {
        let input_currency = swaps[0].input_currency();
        let output_currency = swaps[0].output_currency();
        for Swap { route, .. } in &swaps {
            assert!(input_currency.equals(&route.input), "INPUT_CURRENCY_MATCH");
            assert!(
                output_currency.equals(&route.output),
                "OUTPUT_CURRENCY_MATCH"
            );
        }
        let num_pools = swaps
            .iter()
            .map(|swap| swap.route.pools.len())
            .sum::<usize>();
        let pool_ids = swaps
            .iter()
            .flat_map(|swap| swap.route.pools.iter())
            .map(|pool| {
                Pool::get_pool_id(
                    &pool.currency0,
                    &pool.currency1,
                    pool.fee,
                    pool.tick_spacing,
                    pool.hooks,
                )
                .unwrap()
            });
        let pool_id_set: HashSet<B256> = HashSet::from_iter(pool_ids);
        assert_eq!(num_pools, pool_id_set.len(), "POOLS_DUPLICATED");
        Ok(Self {
            swaps,
            trade_type,
            _input_amount: None,
            _output_amount: None,
            _execution_price: None,
            _price_impact: None,
        })
    }

    /// Creates a trade without computing the result of swapping through the route.
    /// Useful when you have simulated the trade elsewhere and do not have any tick data
    #[inline]
    pub fn create_unchecked_trade(
        route: Route<TInput, TOutput, TP>,
        input_amount: CurrencyAmount<TInput>,
        output_amount: CurrencyAmount<TOutput>,
        trade_type: TradeType,
    ) -> Result<Self, Error> {
        Self::new(
            vec![Swap::new(route, input_amount, output_amount)],
            trade_type,
        )
    }

    /// Creates a trade without computing the result of swapping through the routes.
    /// Useful when you have simulated the trade elsewhere and do not have any tick data
    #[inline]
    pub fn create_unchecked_trade_with_multiple_routes(
        swaps: Vec<Swap<TInput, TOutput, TP>>,
        trade_type: TradeType,
    ) -> Result<Self, Error> {
        Self::new(swaps, trade_type)
    }

    /// When the trade consists of just a single route, this returns the route of the trade.
    #[inline]
    pub fn route(&self) -> &Route<TInput, TOutput, TP> {
        assert_eq!(self.swaps.len(), 1, "MULTIPLE_ROUTES");
        &self.swaps[0].route
    }

    /// Returns the input currency of the swap
    #[inline]
    pub fn input_currency(&self) -> &TInput {
        self.swaps[0].input_currency()
    }

    /// The input amount for the trade assuming no slippage.
    #[inline]
    pub fn input_amount(&self) -> Result<CurrencyAmount<TInput>, Error> {
        let mut total = Fraction::default();
        for Swap { input_amount, .. } in &self.swaps {
            total = total + input_amount.as_fraction();
        }
        CurrencyAmount::from_fractional_amount(
            self.input_currency().clone(),
            total.numerator,
            total.denominator,
        )
        .map_err(Error::Core)
    }

    /// The input amount for the trade assuming no slippage.
    #[inline]
    pub fn input_amount_cached(&mut self) -> Result<CurrencyAmount<TInput>, Error> {
        if let Some(input_amount) = &self._input_amount {
            return Ok(input_amount.clone());
        }
        let input_amount = self.input_amount()?;
        self._input_amount = Some(input_amount.clone());
        Ok(input_amount)
    }

    /// Returns the output currency of the swap
    #[inline]
    pub fn output_currency(&self) -> &TOutput {
        self.swaps[0].output_currency()
    }

    /// The output amount for the trade assuming no slippage.
    #[inline]
    pub fn output_amount(&self) -> Result<CurrencyAmount<TOutput>, Error> {
        let mut total = Fraction::default();
        for Swap { output_amount, .. } in &self.swaps {
            total = total + output_amount.as_fraction();
        }
        CurrencyAmount::from_fractional_amount(
            self.output_currency().clone(),
            total.numerator,
            total.denominator,
        )
        .map_err(Error::Core)
    }

    /// The output amount for the trade assuming no slippage.
    #[inline]
    pub fn output_amount_cached(&mut self) -> Result<CurrencyAmount<TOutput>, Error> {
        if let Some(output_amount) = &self._output_amount {
            return Ok(output_amount.clone());
        }
        let output_amount = self.output_amount()?;
        self._output_amount = Some(output_amount.clone());
        Ok(output_amount)
    }

    /// The price expressed in terms of output amount/input amount.
    #[inline]
    pub fn execution_price(&self) -> Result<Price<TInput, TOutput>, Error> {
        let input_amount = self.input_amount()?;
        let output_amount = self.output_amount()?;
        Ok(Price::from_currency_amounts(input_amount, output_amount))
    }

    /// The price expressed in terms of output amount/input amount.
    #[inline]
    pub fn execution_price_cached(&mut self) -> Result<Price<TInput, TOutput>, Error> {
        if let Some(execution_price) = &self._execution_price {
            return Ok(execution_price.clone());
        }
        let input_amount = self.input_amount_cached()?;
        let output_amount = self.output_amount_cached()?;
        let execution_price = Price::from_currency_amounts(input_amount, output_amount);
        self._execution_price = Some(execution_price.clone());
        Ok(execution_price)
    }

    /// Returns the percent difference between the route's mid price and the price impact
    #[inline]
    pub fn price_impact(&self) -> Result<Percent, Error> {
        let mut spot_output_amount =
            CurrencyAmount::from_raw_amount(self.output_currency().clone(), 0)?;
        for Swap {
            route,
            input_amount,
            ..
        } in &self.swaps
        {
            let mid_price = route.mid_price()?;
            spot_output_amount = spot_output_amount.add(&mid_price.quote(input_amount)?)?;
        }
        let price_impact = spot_output_amount
            .subtract(&self.output_amount()?)?
            .divide(&spot_output_amount)?;
        Ok(Percent::new(
            price_impact.numerator,
            price_impact.denominator,
        ))
    }

    /// Returns the percent difference between the route's mid price and the price impact
    #[inline]
    pub fn price_impact_cached(&mut self) -> Result<Percent, Error> {
        if let Some(price_impact) = &self._price_impact {
            return Ok(price_impact.clone());
        }
        let mut spot_output_amount =
            CurrencyAmount::from_raw_amount(self.output_currency().clone(), 0)?;
        for Swap {
            route,
            input_amount,
            ..
        } in &mut self.swaps
        {
            let mid_price = route.mid_price_cached()?;
            spot_output_amount = spot_output_amount.add(&mid_price.quote(input_amount)?)?;
        }
        let price_impact = spot_output_amount
            .subtract(&self.output_amount_cached()?)?
            .divide(&spot_output_amount)?;
        self._price_impact = Some(Percent::new(
            price_impact.numerator,
            price_impact.denominator,
        ));
        Ok(self._price_impact.clone().unwrap())
    }

    /// Get the minimum amount that must be received from this trade for the given slippage
    /// tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The tolerance of unfavorable slippage from the execution price of
    ///   this trade
    /// * `amount_out`: The amount to receive
    #[inline]
    pub fn minimum_amount_out(
        &self,
        slippage_tolerance: Percent,
        amount_out: Option<CurrencyAmount<TOutput>>,
    ) -> Result<CurrencyAmount<TOutput>, Error> {
        assert!(
            slippage_tolerance >= Percent::default(),
            "SLIPPAGE_TOLERANCE"
        );
        let output_amount = amount_out.unwrap_or(self.output_amount()?);
        if self.trade_type == TradeType::ExactOutput {
            return Ok(output_amount);
        }
        output_amount
            .multiply(&((Percent::new(1, 1) + slippage_tolerance).invert()))
            .map_err(|e| e.into())
    }

    /// Get the minimum amount that must be received from this trade for the given slippage
    /// tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The tolerance of unfavorable slippage from the execution price of
    ///   this trade
    /// * `amount_out`: The amount to receive
    #[inline]
    pub fn minimum_amount_out_cached(
        &mut self,
        slippage_tolerance: Percent,
        amount_out: Option<CurrencyAmount<TOutput>>,
    ) -> Result<CurrencyAmount<TOutput>, Error> {
        assert!(
            slippage_tolerance >= Percent::default(),
            "SLIPPAGE_TOLERANCE"
        );
        let output_amount = amount_out.unwrap_or(self.output_amount_cached()?);
        if self.trade_type == TradeType::ExactOutput {
            return Ok(output_amount);
        }
        output_amount
            .multiply(&((Percent::new(1, 1) + slippage_tolerance).invert()))
            .map_err(|e| e.into())
    }

    /// Get the maximum amount in that can be spent via this trade for the given slippage tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The tolerance of unfavorable slippage from the execution price of
    ///   this trade
    /// * `amount_in`: The amount to spend
    #[inline]
    pub fn maximum_amount_in(
        &self,
        slippage_tolerance: Percent,
        amount_in: Option<CurrencyAmount<TInput>>,
    ) -> Result<CurrencyAmount<TInput>, Error> {
        assert!(
            slippage_tolerance >= Percent::default(),
            "SLIPPAGE_TOLERANCE"
        );
        let amount_in = amount_in.unwrap_or(self.input_amount()?);
        if self.trade_type == TradeType::ExactInput {
            return Ok(amount_in);
        }
        amount_in
            .multiply(&(Percent::new(1, 1) + slippage_tolerance))
            .map_err(|e| e.into())
    }

    /// Get the maximum amount in that can be spent via this trade for the given slippage tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The tolerance of unfavorable slippage from the execution price of
    ///   this trade
    /// * `amount_in`: The amount to spend
    #[inline]
    pub fn maximum_amount_in_cached(
        &mut self,
        slippage_tolerance: Percent,
        amount_in: Option<CurrencyAmount<TInput>>,
    ) -> Result<CurrencyAmount<TInput>, Error> {
        assert!(
            slippage_tolerance >= Percent::default(),
            "SLIPPAGE_TOLERANCE"
        );
        let amount_in = amount_in.unwrap_or(self.input_amount_cached()?);
        if self.trade_type == TradeType::ExactInput {
            return Ok(amount_in);
        }
        amount_in
            .multiply(&(Percent::new(1, 1) + slippage_tolerance))
            .map_err(|e| e.into())
    }

    /// Return the execution price after accounting for slippage tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The allowed tolerated slippage
    #[inline]
    pub fn worst_execution_price(
        &self,
        slippage_tolerance: Percent,
    ) -> Result<Price<TInput, TOutput>, Error> {
        Ok(Price::from_currency_amounts(
            self.maximum_amount_in(slippage_tolerance.clone(), None)?,
            self.minimum_amount_out(slippage_tolerance, None)?,
        ))
    }

    /// Return the execution price after accounting for slippage tolerance
    ///
    /// ## Arguments
    ///
    /// * `slippage_tolerance`: The allowed tolerated slippage
    #[inline]
    pub fn worst_execution_price_cached(
        &mut self,
        slippage_tolerance: Percent,
    ) -> Result<Price<TInput, TOutput>, Error> {
        Ok(Price::from_currency_amounts(
            self.maximum_amount_in_cached(slippage_tolerance.clone(), None)?,
            self.minimum_amount_out_cached(slippage_tolerance, None)?,
        ))
    }
}

impl<TInput, TOutput, TP> Trade<TInput, TOutput, TP>
where
    TInput: BaseCurrency,
    TOutput: BaseCurrency,
    TP: Clone + TickDataProvider,
{
    /// Constructs an exact in trade with the given amount in and route
    ///
    /// ## Arguments
    ///
    /// * `route`: The route of the exact in trade
    /// * `amount_in`: The amount being passed in
    #[inline]
    pub async fn exact_in(
        route: Route<TInput, TOutput, TP>,
        amount_in: CurrencyAmount<impl BaseCurrency>,
    ) -> Result<Self, Error> {
        Self::from_route(route, amount_in, TradeType::ExactInput).await
    }

    /// Constructs an exact out trade with the given amount out and route
    ///
    /// ## Arguments
    ///
    /// * `route`: The route of the exact out trade
    /// * `amount_out`: The amount returned by the trade
    #[inline]
    pub async fn exact_out(
        route: Route<TInput, TOutput, TP>,
        amount_out: CurrencyAmount<impl BaseCurrency>,
    ) -> Result<Self, Error> {
        Self::from_route(route, amount_out, TradeType::ExactOutput).await
    }

    /// Constructs a trade by simulating swaps through the given route
    ///
    /// ## Arguments
    ///
    /// * `route`: The route to swap through
    /// * `amount`: The amount specified, either input or output, depending on `trade_type`
    /// * `trade_type`: Whether the trade is an exact input or exact output swap
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub async fn from_route(
        route: Route<TInput, TOutput, TP>,
        amount: CurrencyAmount<impl BaseCurrency>,
        trade_type: TradeType,
    ) -> Result<Self, Error> {
        let input_amount: CurrencyAmount<TInput>;
        let output_amount: CurrencyAmount<TOutput>;
        match trade_type {
            TradeType::ExactInput => {
                assert!(amount.currency.equals(&route.input), "INPUT");
                // Account for trades that wrap/unwrap as a first step
                let mut token_amount = amount_with_path_currency(&amount, &route.pools[0])?;
                for pool in &route.pools {
                    (token_amount, _) = pool.get_output_amount(&token_amount, None).await?;
                }
                output_amount = CurrencyAmount::from_fractional_amount(
                    route.output.clone(),
                    token_amount.numerator,
                    token_amount.denominator,
                )?;
                input_amount = CurrencyAmount::from_fractional_amount(
                    route.input.clone(),
                    amount.numerator,
                    amount.denominator,
                )?;
            }
            TradeType::ExactOutput => {
                assert!(amount.currency.equals(&route.output), "OUTPUT");
                // Account for trades that wrap/unwrap as a last step
                let mut token_amount =
                    amount_with_path_currency(&amount, route.pools.last().unwrap())?;
                for pool in route.pools.iter().rev() {
                    (token_amount, _) = pool.get_input_amount(&token_amount, None).await?;
                }
                input_amount = CurrencyAmount::from_fractional_amount(
                    route.input.clone(),
                    token_amount.numerator,
                    token_amount.denominator,
                )?;
                output_amount = CurrencyAmount::from_fractional_amount(
                    route.output.clone(),
                    amount.numerator,
                    amount.denominator,
                )?;
            }
        }
        Self::new(
            vec![Swap::new(route, input_amount, output_amount)],
            trade_type,
        )
    }

    /// Constructs a trade from routes by simulating swaps
    ///
    /// ## Arguments
    ///
    /// * `routes`: The routes to swap through and how much of the amount should be routed through
    ///   each
    /// * `trade_type`: Whether the trade is an exact input or exact output swap
    #[inline]
    pub async fn from_routes(
        routes: Vec<(
            CurrencyAmount<impl BaseCurrency>,
            Route<TInput, TOutput, TP>,
        )>,
        trade_type: TradeType,
    ) -> Result<Self, Error> {
        let mut populated_routes: Vec<Swap<TInput, TOutput, TP>> = Vec::with_capacity(routes.len());
        for (amount, route) in routes {
            let trade = Self::from_route(route, amount, trade_type).await?;
            populated_routes.push(trade.swaps.into_iter().next().unwrap());
        }
        Self::new(populated_routes, trade_type)
    }

    /// Given a list of pools, and a fixed amount in, returns the top `max_num_results` trades that
    /// go from an input token amount to an output token, making at most `max_hops` hops.
    ///
    /// ## Note
    ///
    /// This does not consider aggregation, as routes are linear. It's possible a better route
    /// exists by splitting the amount in among multiple routes.
    ///
    /// ## Arguments
    ///
    /// * `pools`: The pools to consider in finding the best trade
    /// * `currency_amount_in`: The exact amount of input currency to spend
    /// * `currency_out`: The desired currency out
    /// * `best_trade_options`: Maximum number of results to return and maximum number of hops a
    ///   returned trade can make, e.g. 1 hop goes through a single pool
    /// * `current_pools`: Used in recursion; the current list of pools
    /// * `next_amount_in`: Used in recursion; the original value of the currency_amount_in
    ///   parameter
    /// * `best_trades`: Used in recursion; the current list of best trades
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub async fn best_trade_exact_in<'a>(
        pools: Vec<Pool<TP>>,
        currency_amount_in: &'a CurrencyAmount<TInput>,
        currency_out: &'a TOutput,
        best_trade_options: BestTradeOptions,
        current_pools: Vec<Pool<TP>>,
        next_amount_in: Option<&'a CurrencyAmount<Currency>>,
        best_trades: &'a mut Vec<Self>,
    ) -> Result<&'a mut Vec<Self>, Error> {
        assert!(!pools.is_empty(), "POOLS");
        let max_num_results = best_trade_options.max_num_results.unwrap_or(3);
        let max_hops = best_trade_options.max_hops.unwrap_or(3);
        assert!(max_hops > 0, "MAX_HOPS");
        if next_amount_in.is_some() {
            assert!(!current_pools.is_empty(), "INVALID_RECURSION");
        }
        for i in 0..pools.len() {
            let pool = &pools[i];
            // pool irrelevant
            match next_amount_in {
                Some(amount_in) => {
                    if !pool.involves_token(&amount_in.currency) {
                        continue;
                    }
                }
                None => {
                    if !pool.involves_token(&currency_amount_in.currency) {
                        continue;
                    }
                }
            }
            let amount_out = match next_amount_in {
                Some(amount_in) => pool.get_output_amount(amount_in, None).await,
                None => pool.get_output_amount(currency_amount_in, None).await,
            };
            let amount_out = match amount_out {
                Ok((amount_out, _)) => amount_out,
                Err(Error::InsufficientLiquidity) => continue,
                Err(e) => return Err(e),
            };
            // we have arrived at the output token, so this is the final trade of one of the paths
            if amount_out.currency.equals(currency_out) {
                let mut next_pools = current_pools.clone();
                next_pools.push(pool.clone());
                let trade = Self::from_route(
                    Route::new(
                        next_pools,
                        currency_amount_in.currency.clone(),
                        currency_out.clone(),
                    )?,
                    currency_amount_in.clone(),
                    TradeType::ExactInput,
                )
                .await?;
                sorted_insert(best_trades, trade, max_num_results, trade_comparator);
            } else if max_hops > 1 && pools.len() > 1 {
                let pools_excluding_this_pool = pools[..i]
                    .iter()
                    .chain(pools[i + 1..].iter())
                    .cloned()
                    .collect();
                // otherwise, consider all the other paths that lead from this token as long as we
                // have not exceeded maxHops
                let mut next_pools = current_pools.clone();
                next_pools.push(pool.clone());
                Box::pin(Self::best_trade_exact_in(
                    pools_excluding_this_pool,
                    currency_amount_in,
                    currency_out,
                    BestTradeOptions {
                        max_num_results: Some(max_num_results),
                        max_hops: Some(max_hops - 1),
                    },
                    next_pools,
                    Some(&amount_out),
                    best_trades,
                ))
                .await?;
            }
        }
        Ok(best_trades)
    }

    /// Given a list of pools, and a fixed amount out, returns the top `max_num_results` trades that
    /// go from an input token to an output token amount, making at most `max_hops` hops.
    ///
    /// ## Note
    ///
    /// This does not consider aggregation, as routes are linear. It's possible a better route
    /// exists by splitting the amount in among multiple routes.
    ///
    /// ## Arguments
    ///
    /// * `pools`: The pools to consider in finding the best trade
    /// * `currency_in`: The currency to spend
    /// * `currency_amount_out`: The desired currency amount out
    /// * `best_trade_options`: Maximum number of results to return and maximum number of hops a
    ///   returned trade can make, e.g. 1 hop goes through a single pool
    /// * `current_pools`: Used in recursion; the current list of pools
    /// * `next_amount_out`: Used in recursion; the exact amount of currency out
    /// * `best_trades`: Used in recursion; the current list of best trades
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub async fn best_trade_exact_out<'a>(
        pools: Vec<Pool<TP>>,
        currency_in: &'a TInput,
        currency_amount_out: &'a CurrencyAmount<TOutput>,
        best_trade_options: BestTradeOptions,
        current_pools: Vec<Pool<TP>>,
        next_amount_out: Option<&'a CurrencyAmount<Currency>>,
        best_trades: &'a mut Vec<Self>,
    ) -> Result<&'a mut Vec<Self>, Error> {
        assert!(!pools.is_empty(), "POOLS");
        let max_num_results = best_trade_options.max_num_results.unwrap_or(3);
        let max_hops = best_trade_options.max_hops.unwrap_or(3);
        assert!(max_hops > 0, "MAX_HOPS");
        if next_amount_out.is_some() {
            assert!(!current_pools.is_empty(), "INVALID_RECURSION");
        }
        for i in 0..pools.len() {
            let pool = &pools[i];
            // pool irrelevant
            match next_amount_out {
                Some(amount_out) => {
                    if !pool.involves_token(&amount_out.currency) {
                        continue;
                    }
                }
                None => {
                    if !pool.involves_token(&currency_amount_out.currency) {
                        continue;
                    }
                }
            }
            let amount_in = match next_amount_out {
                Some(amount_out) => pool.get_input_amount(amount_out, None).await,
                None => pool.get_input_amount(currency_amount_out, None).await,
            };
            let amount_in = match amount_in {
                Ok((amount_in, _)) => amount_in,
                Err(Error::InsufficientLiquidity) => continue,
                Err(e) => return Err(e),
            };
            // we have arrived at the input token, so this is the first trade of one of the paths
            if amount_in.currency.equals(currency_in) {
                let mut next_pools = vec![pool.clone()];
                next_pools.extend(current_pools.clone());
                let trade = Self::from_route(
                    Route::new(
                        next_pools,
                        currency_in.clone(),
                        currency_amount_out.currency.clone(),
                    )?,
                    currency_amount_out.clone(),
                    TradeType::ExactOutput,
                )
                .await?;
                sorted_insert(best_trades, trade, max_num_results, trade_comparator);
            } else if max_hops > 1 && pools.len() > 1 {
                let pools_excluding_this_pool = pools[..i]
                    .iter()
                    .chain(pools[i + 1..].iter())
                    .cloned()
                    .collect();
                // otherwise, consider all the other paths that arrive at this token as long as we
                // have not exceeded maxHops
                let mut next_pools = vec![pool.clone()];
                next_pools.extend(current_pools.clone());
                Box::pin(Self::best_trade_exact_out(
                    pools_excluding_this_pool,
                    currency_in,
                    currency_amount_out,
                    BestTradeOptions {
                        max_num_results: Some(max_num_results),
                        max_hops: Some(max_hops - 1),
                    },
                    next_pools,
                    Some(&amount_in),
                    best_trades,
                ))
                .await?;
            }
        }
        Ok(best_trades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_route, currency_amount, tests::*, trade_from_route};
    use num_integer::Roots;
    use num_traits::ToPrimitive;
    use once_cell::sync::Lazy;
    use tokio::sync::OnceCell;

    fn v2_style_pool(
        reserve0: CurrencyAmount<Currency>,
        reserve1: CurrencyAmount<Currency>,
        fee_amount: Option<FeeAmount>,
    ) -> Pool<TickListDataProvider> {
        let fee_amount = fee_amount.unwrap_or(FeeAmount::MEDIUM);
        let sqrt_ratio_x96 = encode_sqrt_ratio_x96(reserve1.quotient(), reserve0.quotient());
        let liquidity = (reserve0.quotient() * reserve1.quotient())
            .sqrt()
            .to_u128()
            .unwrap();
        let tick_spacing = 60;
        Pool::new_with_tick_data_provider(
            reserve0.meta.currency,
            reserve1.meta.currency,
            fee_amount.into(),
            tick_spacing,
            Address::ZERO,
            sqrt_ratio_x96,
            liquidity,
            TickListDataProvider::new(
                vec![
                    Tick::new(
                        nearest_usable_tick(MIN_TICK_I32, tick_spacing),
                        liquidity,
                        liquidity as i128,
                    ),
                    Tick::new(
                        nearest_usable_tick(MAX_TICK_I32, tick_spacing),
                        liquidity,
                        -(liquidity as i128),
                    ),
                ],
                tick_spacing,
            ),
        )
        .unwrap()
    }

    static POOL_0_1: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(TOKEN0.clone()), 100000),
            currency_amount!(Currency::from(TOKEN1.clone()), 100000),
            None,
        )
    });
    static POOL_0_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(TOKEN0.clone()), 100000),
            currency_amount!(Currency::from(TOKEN2.clone()), 110000),
            None,
        )
    });
    static POOL_0_3: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(TOKEN0.clone()), 100000),
            currency_amount!(Currency::from(TOKEN3.clone()), 90000),
            None,
        )
    });
    static POOL_1_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(TOKEN1.clone()), 120000),
            currency_amount!(Currency::from(TOKEN2.clone()), 100000),
            None,
        )
    });
    static POOL_1_3: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(TOKEN1.clone()), 120000),
            currency_amount!(Currency::from(TOKEN3.clone()), 130000),
            None,
        )
    });
    static POOL_ETH_0: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(ETHER.clone()), 100000),
            currency_amount!(Currency::from(TOKEN0.clone()), 100000),
            None,
        )
    });
    static POOL_ETH_1: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(ETHER.clone()), 100000),
            currency_amount!(Currency::from(TOKEN1.clone()), 100000),
            None,
        )
    });
    static POOL_ETH_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(ETHER.clone()), 100000),
            currency_amount!(Currency::from(TOKEN2.clone()), 100000),
            None,
        )
    });
    static POOL_WETH_0: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            currency_amount!(Currency::from(WETH.clone()), 100000),
            currency_amount!(Currency::from(TOKEN0.clone()), 100000),
            None,
        )
    });

    mod from_route {
        use super::*;

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_input() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, ETHER, TOKEN0),
                currency_amount!(ETHER, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_input_on_a_weth_pool() {
            let trade = trade_from_route!(
                create_route!(POOL_WETH_0, ETHER, TOKEN0),
                currency_amount!(ETHER, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_weth_as_input_on_a_eth_pool() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, WETH, TOKEN0),
                currency_amount!(WETH, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), WETH.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_output_on_a_weth_pool() {
            let trade = trade_from_route!(
                create_route!(POOL_WETH_0, TOKEN0, ETHER),
                currency_amount!(TOKEN0, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_weth_as_output_on_a_eth_pool() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, TOKEN0, WETH),
                currency_amount!(TOKEN0, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), WETH.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_input_for_exact_output() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, ETHER, TOKEN0),
                currency_amount!(TOKEN0, 10000),
                TradeType::ExactOutput
            );
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_output() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, TOKEN0, ETHER),
                currency_amount!(ETHER, 10000),
                TradeType::ExactOutput
            );
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_output_for_exact_input() {
            let trade = trade_from_route!(
                create_route!(POOL_ETH_0, TOKEN0, ETHER),
                currency_amount!(TOKEN0, 10000),
                TradeType::ExactInput
            );
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }
    }

    mod from_routes {
        use super::*;

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_input_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![(
                    currency_amount!(ETHER, 10000),
                    create_route!(POOL_ETH_0, ETHER, TOKEN0),
                )],
                TradeType::ExactInput,
            )
            .await
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_input_for_exact_output_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        currency_amount!(TOKEN0, 3000),
                        create_route!(POOL_ETH_0, ETHER, TOKEN0),
                    ),
                    (
                        currency_amount!(TOKEN0, 7000),
                        create_route!(POOL_ETH_1, POOL_0_1; ETHER, TOKEN0),
                    ),
                ],
                TradeType::ExactOutput,
            )
            .await
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_output_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        currency_amount!(ETHER, 4000),
                        create_route!(POOL_ETH_0, TOKEN0, ETHER),
                    ),
                    (
                        currency_amount!(ETHER, 6000),
                        create_route!(POOL_0_1, POOL_ETH_1; TOKEN0, ETHER),
                    ),
                ],
                TradeType::ExactOutput,
            )
            .await
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[tokio::test]
        async fn can_be_constructed_with_ether_as_output_for_exact_input_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        currency_amount!(TOKEN0, 3000),
                        create_route!(POOL_ETH_0, TOKEN0, ETHER),
                    ),
                    (
                        currency_amount!(TOKEN0, 7000),
                        create_route!(POOL_0_1, POOL_ETH_1; TOKEN0, ETHER),
                    ),
                ],
                TradeType::ExactInput,
            )
            .await
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[tokio::test]
        #[should_panic(expected = "POOLS_DUPLICATED")]
        async fn throws_if_pools_are_reused_between_routes() {
            let _ = Trade::from_routes(
                vec![
                    (
                        currency_amount!(TOKEN0, 4500),
                        create_route!(POOL_0_1, POOL_ETH_1; TOKEN0, ETHER),
                    ),
                    (
                        currency_amount!(TOKEN0, 5500),
                        create_route!(POOL_0_1, POOL_1_2, POOL_ETH_2; TOKEN0, ETHER),
                    ),
                ],
                TradeType::ExactInput,
            )
            .await
            .unwrap();
        }
    }

    mod create_unchecked_trade {
        use super::*;

        #[test]
        #[should_panic(expected = "INPUT_CURRENCY_MATCH")]
        fn throws_if_input_currency_does_not_match_route() {
            Trade::create_unchecked_trade(
                create_route!(POOL_0_1, TOKEN0, TOKEN1),
                currency_amount!(TOKEN2, 10000),
                currency_amount!(TOKEN1, 10000),
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        #[should_panic(expected = "OUTPUT_CURRENCY_MATCH")]
        fn throws_if_output_currency_does_not_match_route() {
            Trade::create_unchecked_trade(
                create_route!(POOL_0_1, TOKEN0, TOKEN1),
                currency_amount!(TOKEN0, 10000),
                currency_amount!(TOKEN2, 10000),
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        fn can_create_an_exact_input_trade_without_simulating() {
            Trade::create_unchecked_trade(
                create_route!(POOL_0_1, TOKEN0, TOKEN1),
                currency_amount!(TOKEN0, 10000),
                currency_amount!(TOKEN1, 100000),
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        fn can_create_an_exact_output_trade_without_simulating() {
            Trade::create_unchecked_trade(
                create_route!(POOL_0_1, TOKEN0, TOKEN1),
                currency_amount!(TOKEN0, 10000),
                currency_amount!(TOKEN1, 100000),
                TradeType::ExactOutput,
            )
            .unwrap();
        }
    }

    mod create_unchecked_trade_with_multiple_routes {
        use super::*;

        #[test]
        #[should_panic(expected = "INPUT_CURRENCY_MATCH")]
        fn throws_if_input_currency_does_not_match_route_with_multiple_routes() {
            Trade::create_unchecked_trade_with_multiple_routes(
                vec![
                    Swap::new(
                        create_route!(POOL_1_2, TOKEN2, TOKEN1),
                        currency_amount!(TOKEN2, 2000),
                        currency_amount!(TOKEN1, 2000),
                    ),
                    Swap::new(
                        create_route!(POOL_0_1, TOKEN0, TOKEN1),
                        currency_amount!(TOKEN2, 8000),
                        currency_amount!(TOKEN1, 8000),
                    ),
                ],
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        #[should_panic(expected = "OUTPUT_CURRENCY_MATCH")]
        fn throws_if_output_currency_does_not_match_route_with_multiple_routes() {
            Trade::create_unchecked_trade_with_multiple_routes(
                vec![
                    Swap::new(
                        create_route!(POOL_0_2, TOKEN0, TOKEN2),
                        currency_amount!(TOKEN0, 10000),
                        currency_amount!(TOKEN2, 10000),
                    ),
                    Swap::new(
                        create_route!(POOL_0_1, TOKEN0, TOKEN1),
                        currency_amount!(TOKEN0, 10000),
                        currency_amount!(TOKEN2, 10000),
                    ),
                ],
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        fn can_create_an_exact_input_trade_without_simulating_with_multiple_routes() {
            Trade::create_unchecked_trade_with_multiple_routes(
                vec![
                    Swap::new(
                        create_route!(POOL_0_1, TOKEN0, TOKEN1),
                        currency_amount!(TOKEN0, 5000),
                        currency_amount!(TOKEN1, 50000),
                    ),
                    Swap::new(
                        create_route!(POOL_0_2, POOL_1_2; TOKEN0, TOKEN1),
                        currency_amount!(TOKEN0, 5000),
                        currency_amount!(TOKEN1, 50000),
                    ),
                ],
                TradeType::ExactInput,
            )
            .unwrap();
        }

        #[test]
        fn can_create_an_exact_output_trade_without_simulating_with_multiple_routes() {
            Trade::create_unchecked_trade_with_multiple_routes(
                vec![
                    Swap::new(
                        create_route!(POOL_0_1, TOKEN0, TOKEN1),
                        currency_amount!(TOKEN0, 5001),
                        currency_amount!(TOKEN1, 50000),
                    ),
                    Swap::new(
                        create_route!(POOL_0_2, POOL_1_2; TOKEN0, TOKEN1),
                        currency_amount!(TOKEN0, 4999),
                        currency_amount!(TOKEN1, 50000),
                    ),
                ],
                TradeType::ExactOutput,
            )
            .unwrap();
        }
    }

    mod route_and_swaps {
        use super::*;

        #[test]
        fn can_access_route_for_single_route_trade_if_less_than_0() {
            let route = create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2);
            let trade = Trade::create_unchecked_trade(
                route.clone(),
                currency_amount!(TOKEN0, 100),
                currency_amount!(TOKEN2, 69),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.route(), &route);
        }

        static MULTI_ROUTE: Lazy<Trade<Token, Token, TickListDataProvider>> = Lazy::new(|| {
            Trade::create_unchecked_trade_with_multiple_routes(
                vec![
                    Swap::new(
                        create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                        currency_amount!(TOKEN0, 50),
                        currency_amount!(TOKEN2, 35),
                    ),
                    Swap::new(
                        create_route!(POOL_0_2, TOKEN0, TOKEN2),
                        currency_amount!(TOKEN0, 50),
                        currency_amount!(TOKEN2, 34),
                    ),
                ],
                TradeType::ExactInput,
            )
            .unwrap()
        });

        #[test]
        fn can_access_routes_for_both_single_and_multi_route_trades() {
            assert_eq!(MULTI_ROUTE.swaps.len(), 2);
        }

        #[test]
        #[should_panic(expected = "MULTIPLE_ROUTES")]
        fn throws_if_access_route_on_multi_route_trade() {
            let _ = MULTI_ROUTE.route();
        }
    }

    mod worst_execution_price {
        use super::*;

        mod exact_input {
            use super::*;

            static EXACT_IN: Lazy<Trade<Token, Token, TickListDataProvider>> = Lazy::new(|| {
                Trade::create_unchecked_trade(
                    create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                    currency_amount!(TOKEN0, 100),
                    currency_amount!(TOKEN2, 69),
                    TradeType::ExactInput,
                )
                .unwrap()
            });
            static EXACT_IN_MULTI_ROUTE: Lazy<Trade<Token, Token, TickListDataProvider>> =
                Lazy::new(|| {
                    Trade::create_unchecked_trade_with_multiple_routes(
                        vec![
                            Swap::new(
                                create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 50),
                                currency_amount!(TOKEN2, 35),
                            ),
                            Swap::new(
                                create_route!(POOL_0_2, TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 50),
                                currency_amount!(TOKEN2, 34),
                            ),
                        ],
                        TradeType::ExactInput,
                    )
                    .unwrap()
                });

            #[test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            fn throws_if_less_than_0() {
                let _ = EXACT_IN
                    .worst_execution_price(Percent::new(-1, 100))
                    .unwrap();
            }

            #[test]
            fn returns_exact_if_0() {
                assert_eq!(
                    EXACT_IN
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    EXACT_IN.execution_price().unwrap()
                );
            }

            #[test]
            fn returns_exact_if_nonzero() {
                assert_eq!(
                    EXACT_IN
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 100, 69)
                );
                assert_eq!(
                    EXACT_IN
                        .worst_execution_price(Percent::new(5, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 10500, 6900)
                );
                assert_eq!(
                    EXACT_IN
                        .worst_execution_price(Percent::new(200, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 100, 23)
                );
            }

            #[test]
            fn returns_exact_if_nonzero_with_multiple_routes() {
                assert_eq!(
                    EXACT_IN_MULTI_ROUTE
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 100, 69)
                );
                assert_eq!(
                    EXACT_IN_MULTI_ROUTE
                        .worst_execution_price(Percent::new(5, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 10500, 6900)
                );
                assert_eq!(
                    EXACT_IN_MULTI_ROUTE
                        .worst_execution_price(Percent::new(200, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 100, 23)
                );
            }
        }

        mod exact_output {
            use super::*;

            static EXACT_OUT: Lazy<Trade<Token, Token, TickListDataProvider>> = Lazy::new(|| {
                Trade::create_unchecked_trade(
                    create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                    currency_amount!(TOKEN0, 156),
                    currency_amount!(TOKEN2, 100),
                    TradeType::ExactOutput,
                )
                .unwrap()
            });
            static EXACT_OUT_MULTI_ROUTE: Lazy<Trade<Token, Token, TickListDataProvider>> =
                Lazy::new(|| {
                    Trade::create_unchecked_trade_with_multiple_routes(
                        vec![
                            Swap::new(
                                create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 78),
                                currency_amount!(TOKEN2, 50),
                            ),
                            Swap::new(
                                create_route!(POOL_0_2, TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 78),
                                currency_amount!(TOKEN2, 50),
                            ),
                        ],
                        TradeType::ExactOutput,
                    )
                    .unwrap()
                });

            #[test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            fn throws_if_less_than_0() {
                let _ = EXACT_OUT
                    .worst_execution_price(Percent::new(-1, 100))
                    .unwrap();
            }

            #[test]
            fn returns_exact_if_0() {
                assert_eq!(
                    EXACT_OUT
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    EXACT_OUT.execution_price().unwrap()
                );
            }

            #[test]
            fn returns_exact_if_nonzero() {
                assert_eq!(
                    EXACT_OUT
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 156, 100)
                );
                assert_eq!(
                    EXACT_OUT
                        .worst_execution_price(Percent::new(5, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 16380, 10000)
                );
                assert_eq!(
                    EXACT_OUT
                        .worst_execution_price(Percent::new(200, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 468, 100)
                );
            }

            #[test]
            fn returns_exact_if_nonzero_with_multiple_routes() {
                assert_eq!(
                    EXACT_OUT_MULTI_ROUTE
                        .worst_execution_price(Percent::new(0, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 156, 100)
                );
                assert_eq!(
                    EXACT_OUT_MULTI_ROUTE
                        .worst_execution_price(Percent::new(5, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 16380, 10000)
                );
                assert_eq!(
                    EXACT_OUT_MULTI_ROUTE
                        .worst_execution_price(Percent::new(200, 100))
                        .unwrap(),
                    Price::new(TOKEN0.clone(), TOKEN2.clone(), 468, 100)
                );
            }
        }
    }

    mod price_impact {
        use super::*;

        mod exact_input {
            use super::*;

            static EXACT_IN: Lazy<Trade<Token, Token, TickListDataProvider>> = Lazy::new(|| {
                Trade::create_unchecked_trade_with_multiple_routes(
                    vec![Swap::new(
                        create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                        currency_amount!(TOKEN0, 100),
                        currency_amount!(TOKEN2, 69),
                    )],
                    TradeType::ExactInput,
                )
                .unwrap()
            });
            static EXACT_IN_MULTI_ROUTES: Lazy<Trade<Token, Token, TickListDataProvider>> =
                Lazy::new(|| {
                    Trade::create_unchecked_trade_with_multiple_routes(
                        vec![
                            Swap::new(
                                create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 90),
                                currency_amount!(TOKEN2, 62),
                            ),
                            Swap::new(
                                create_route!(POOL_0_2, TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 10),
                                currency_amount!(TOKEN2, 7),
                            ),
                        ],
                        TradeType::ExactInput,
                    )
                    .unwrap()
                });

            #[test]
            fn is_cached() {
                let mut trade = EXACT_IN.clone();
                assert_eq!(
                    trade.price_impact_cached().unwrap(),
                    trade._price_impact.unwrap()
                );
            }

            #[test]
            fn is_correct() {
                assert_eq!(
                    EXACT_IN
                        .price_impact()
                        .unwrap()
                        .to_significant(3, None)
                        .unwrap(),
                    "17.2"
                );
            }

            #[test]
            fn is_cached_with_multiple_routes() {
                let mut trade = EXACT_IN_MULTI_ROUTES.clone();
                assert_eq!(
                    trade.price_impact_cached().unwrap(),
                    trade._price_impact.unwrap()
                );
            }

            #[test]
            fn is_correct_with_multiple_routes() {
                assert_eq!(
                    EXACT_IN_MULTI_ROUTES
                        .price_impact()
                        .unwrap()
                        .to_significant(3, None)
                        .unwrap(),
                    "19.8"
                );
            }
        }

        mod exact_output {
            use super::*;

            static EXACT_OUT: Lazy<Trade<Token, Token, TickListDataProvider>> = Lazy::new(|| {
                Trade::create_unchecked_trade_with_multiple_routes(
                    vec![Swap::new(
                        create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                        currency_amount!(TOKEN0, 156),
                        currency_amount!(TOKEN2, 100),
                    )],
                    TradeType::ExactOutput,
                )
                .unwrap()
            });
            static EXACT_OUT_MULTI_ROUTES: Lazy<Trade<Token, Token, TickListDataProvider>> =
                Lazy::new(|| {
                    Trade::create_unchecked_trade_with_multiple_routes(
                        vec![
                            Swap::new(
                                create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 140),
                                currency_amount!(TOKEN2, 90),
                            ),
                            Swap::new(
                                create_route!(POOL_0_2, TOKEN0, TOKEN2),
                                currency_amount!(TOKEN0, 16),
                                currency_amount!(TOKEN2, 10),
                            ),
                        ],
                        TradeType::ExactOutput,
                    )
                    .unwrap()
                });

            #[test]
            fn is_cached() {
                let mut trade = EXACT_OUT.clone();
                assert_eq!(
                    trade.price_impact_cached().unwrap(),
                    trade._price_impact.unwrap()
                );
            }

            #[test]
            fn is_correct() {
                assert_eq!(
                    EXACT_OUT
                        .price_impact()
                        .unwrap()
                        .to_significant(3, None)
                        .unwrap(),
                    "23.1"
                );
            }

            #[test]
            fn is_cached_with_multiple_routes() {
                let mut trade = EXACT_OUT_MULTI_ROUTES.clone();
                assert_eq!(
                    trade.price_impact_cached().unwrap(),
                    trade._price_impact.unwrap()
                );
            }

            #[test]
            fn is_correct_with_multiple_routes() {
                assert_eq!(
                    EXACT_OUT_MULTI_ROUTES
                        .price_impact()
                        .unwrap()
                        .to_significant(3, None)
                        .unwrap(),
                    "25.5"
                );
            }
        }
    }

    mod best_trade_exact_in {
        use super::*;

        #[tokio::test]
        #[should_panic(expected = "POOLS")]
        async fn throws_with_empty_pools() {
            let _ = Trade::<Token, Token, NoTickDataProvider>::best_trade_exact_in(
                vec![],
                &currency_amount!(TOKEN0, 10000),
                &TOKEN2.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut vec![],
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        #[should_panic(expected = "MAX_HOPS")]
        async fn throws_with_max_hops_of_0() {
            let _ = Trade::best_trade_exact_in(
                vec![POOL_0_2.clone()],
                &currency_amount!(TOKEN0, 10000),
                &TOKEN2.clone(),
                BestTradeOptions {
                    max_hops: Some(0),
                    ..Default::default()
                },
                vec![],
                None,
                &mut vec![],
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        async fn provides_best_route() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &currency_amount!(TOKEN0, 10000),
                &TOKEN2.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].swaps[0].route.pools.len(), 1); // 0 -> 2 at 10:11
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![TOKEN0.clone().into(), TOKEN2.clone().into()]
            );
            assert_eq!(
                result[0].input_amount().unwrap(),
                currency_amount!(TOKEN0, 10000)
            );
            assert_eq!(
                result[0].output_amount().unwrap(),
                currency_amount!(TOKEN2, 9971)
            );
            assert_eq!(result[1].swaps[0].route.pools.len(), 2); // 0 -> 1 -> 2 at 12:12:10
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    TOKEN0.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN2.clone().into()
                ]
            );
            assert_eq!(
                result[1].input_amount().unwrap(),
                currency_amount!(TOKEN0, 10000)
            );
            assert_eq!(
                result[1].output_amount().unwrap(),
                currency_amount!(TOKEN2, 7004)
            );
        }

        #[tokio::test]
        async fn respects_max_hops() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &currency_amount!(TOKEN0, 10),
                &TOKEN2.clone(),
                BestTradeOptions {
                    max_hops: Some(1),
                    ..Default::default()
                },
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].swaps[0].route.pools.len(), 1); // 0 -> 2 at 10:11
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![TOKEN0.clone().into(), TOKEN2.clone().into()]
            );
        }

        #[tokio::test]
        async fn insufficient_input_for_one_pool() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &currency_amount!(TOKEN0, 1),
                &TOKEN2.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].swaps[0].route.pools.len(), 1); // 0 -> 2 at 10:11
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![TOKEN0.clone().into(), TOKEN2.clone().into()]
            );
            assert_eq!(
                result[0].output_amount().unwrap(),
                currency_amount!(TOKEN2, 0)
            );
        }

        #[tokio::test]
        async fn respects_max_num_results() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &currency_amount!(TOKEN0, 10),
                &TOKEN2.clone(),
                BestTradeOptions {
                    max_num_results: Some(1),
                    ..Default::default()
                },
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 1);
        }

        #[tokio::test]
        async fn no_path() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![POOL_0_1.clone(), POOL_0_3.clone(), POOL_1_3.clone()],
                &currency_amount!(TOKEN0, 10),
                &TOKEN2.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 0);
        }

        #[tokio::test]
        async fn works_for_ether_currency_input() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![
                    POOL_ETH_0.clone(),
                    POOL_0_1.clone(),
                    POOL_0_3.clone(),
                    POOL_1_3.clone(),
                ],
                &currency_amount!(ETHER, 100),
                &TOKEN3.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].input_currency().clone(), ETHER.clone());
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![
                    ETHER.clone().into(),
                    TOKEN0.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN3.clone().into()
                ]
            );
            assert_eq!(result[0].output_currency().clone(), TOKEN3.clone());
            assert_eq!(result[1].input_currency().clone(), ETHER.clone());
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    ETHER.clone().into(),
                    TOKEN0.clone().into(),
                    TOKEN3.clone().into()
                ]
            );
            assert_eq!(result[1].output_currency().clone(), TOKEN3.clone());
        }

        #[tokio::test]
        async fn works_for_ether_currency_output() {
            let mut result = vec![];
            Trade::best_trade_exact_in(
                vec![
                    POOL_ETH_0.clone(),
                    POOL_0_1.clone(),
                    POOL_0_3.clone(),
                    POOL_1_3.clone(),
                ],
                &currency_amount!(TOKEN3, 100),
                &ETHER.clone(),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].input_currency().clone(), TOKEN3.clone());
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![
                    TOKEN3.clone().into(),
                    TOKEN0.clone().into(),
                    ETHER.clone().into()
                ]
            );
            assert_eq!(result[0].output_currency().clone(), ETHER.clone());
            assert_eq!(result[1].input_currency().clone(), TOKEN3.clone());
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    TOKEN3.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN0.clone().into(),
                    ETHER.clone().into()
                ]
            );
            assert_eq!(result[1].output_currency().clone(), ETHER.clone());
        }
    }

    mod maximum_amount_in {
        use super::*;

        mod exact_input {
            use super::*;

            static EXACT_IN: OnceCell<Trade<Token, Token, TickListDataProvider>> =
                OnceCell::const_new();

            async fn get_exact_in() -> &'static Trade<Token, Token, TickListDataProvider> {
                EXACT_IN
                    .get_or_init(|| async {
                        trade_from_route!(
                            create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                            currency_amount!(TOKEN0, 100),
                            TradeType::ExactInput
                        )
                    })
                    .await
            }

            #[tokio::test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            async fn throws_if_less_than_0() {
                let trade = get_exact_in().await;
                let _ = trade.maximum_amount_in(Percent::new(-1, 100), None);
            }

            #[tokio::test]
            async fn returns_exact_if_0() {
                let trade = get_exact_in().await;
                assert_eq!(
                    trade.maximum_amount_in(Percent::new(0, 100), None).unwrap(),
                    trade.input_amount().unwrap()
                );
            }

            #[tokio::test]
            async fn returns_exact_if_nonzero() {
                let trade = get_exact_in().await;
                assert_eq!(
                    trade.maximum_amount_in(Percent::new(0, 100), None).unwrap(),
                    currency_amount!(TOKEN0, 100)
                );
                assert_eq!(
                    trade.maximum_amount_in(Percent::new(5, 100), None).unwrap(),
                    currency_amount!(TOKEN0, 100)
                );
                assert_eq!(
                    trade
                        .maximum_amount_in(Percent::new(200, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN0, 100)
                );
            }
        }

        mod exact_output {
            use super::*;

            static EXACT_OUT: OnceCell<Trade<Token, Token, TickListDataProvider>> =
                OnceCell::const_new();

            async fn get_exact_out() -> &'static Trade<Token, Token, TickListDataProvider> {
                EXACT_OUT
                    .get_or_init(|| async {
                        trade_from_route!(
                            create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                            currency_amount!(TOKEN2, 10000),
                            TradeType::ExactOutput
                        )
                    })
                    .await
            }

            #[tokio::test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            async fn throws_if_less_than_0() {
                let trade = get_exact_out().await;
                let _ = trade.maximum_amount_in(Percent::new(-1, 10000), None);
            }

            #[tokio::test]
            async fn returns_exact_if_0() {
                let trade = get_exact_out().await;
                assert_eq!(
                    trade
                        .maximum_amount_in(Percent::new(0, 10000), None)
                        .unwrap(),
                    trade.input_amount().unwrap()
                );
            }

            #[tokio::test]
            async fn returns_exact_if_nonzero() {
                let trade = get_exact_out().await;
                assert_eq!(
                    trade
                        .maximum_amount_in(Percent::new(0, 10000), None)
                        .unwrap(),
                    currency_amount!(TOKEN0, 15488)
                );
                assert_eq!(
                    trade.maximum_amount_in(Percent::new(5, 100), None).unwrap(),
                    CurrencyAmount::from_fractional_amount(TOKEN0.clone(), 1626240, 100).unwrap()
                );
                assert_eq!(
                    trade
                        .maximum_amount_in(Percent::new(200, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN0, 46464)
                );
            }
        }
    }

    mod minimum_amount_out {
        use super::*;

        mod exact_input {
            use super::*;

            static EXACT_IN: OnceCell<Trade<Token, Token, TickListDataProvider>> =
                OnceCell::const_new();

            async fn get_exact_in() -> &'static Trade<Token, Token, TickListDataProvider> {
                EXACT_IN
                    .get_or_init(|| async {
                        trade_from_route!(
                            create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                            currency_amount!(TOKEN0, 10000),
                            TradeType::ExactInput
                        )
                    })
                    .await
            }

            #[tokio::test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            async fn throws_if_less_than_0() {
                let trade = get_exact_in().await;
                let _ = trade.minimum_amount_out(Percent::new(-1, 100), None);
            }

            #[tokio::test]
            async fn returns_exact_if_0() {
                let trade = get_exact_in().await;
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(0, 10000), None)
                        .unwrap(),
                    trade.output_amount().unwrap()
                );
            }

            #[tokio::test]
            async fn returns_exact_if_nonzero() {
                let trade = get_exact_in().await;
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(0, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN2, 7004)
                );
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(5, 100), None)
                        .unwrap(),
                    CurrencyAmount::from_fractional_amount(TOKEN2.clone(), 700400, 105).unwrap()
                );
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(200, 100), None)
                        .unwrap(),
                    CurrencyAmount::from_fractional_amount(TOKEN2.clone(), 700400, 300).unwrap()
                );
            }
        }

        mod exact_output {
            use super::*;

            static EXACT_OUT: OnceCell<Trade<Token, Token, TickListDataProvider>> =
                OnceCell::const_new();

            async fn get_exact_out() -> &'static Trade<Token, Token, TickListDataProvider> {
                EXACT_OUT
                    .get_or_init(|| async {
                        trade_from_route!(
                            create_route!(POOL_0_1, POOL_1_2; TOKEN0, TOKEN2),
                            currency_amount!(TOKEN2, 100),
                            TradeType::ExactOutput
                        )
                    })
                    .await
            }

            #[tokio::test]
            #[should_panic(expected = "SLIPPAGE_TOLERANCE")]
            async fn throws_if_less_than_0() {
                let trade = get_exact_out().await;
                let _ = trade.minimum_amount_out(Percent::new(-1, 100), None);
            }

            #[tokio::test]
            async fn returns_exact_if_0() {
                let trade = get_exact_out().await;
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(0, 100), None)
                        .unwrap(),
                    trade.output_amount().unwrap()
                );
            }

            #[tokio::test]
            async fn returns_exact_if_nonzero() {
                let trade = get_exact_out().await;
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(0, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN2, 100)
                );
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(5, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN2, 100)
                );
                assert_eq!(
                    trade
                        .minimum_amount_out(Percent::new(200, 100), None)
                        .unwrap(),
                    currency_amount!(TOKEN2, 100)
                );
            }
        }
    }

    mod best_trade_exact_out {
        use super::*;

        #[tokio::test]
        #[should_panic(expected = "POOLS")]
        async fn throws_with_empty_pools() {
            let _ = Trade::<Token, Token, NoTickDataProvider>::best_trade_exact_out(
                vec![],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 100),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut vec![],
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        #[should_panic(expected = "MAX_HOPS")]
        async fn throws_with_max_hops_of_0() {
            let _ = Trade::best_trade_exact_out(
                vec![POOL_0_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 100),
                BestTradeOptions {
                    max_hops: Some(0),
                    ..Default::default()
                },
                vec![],
                None,
                &mut vec![],
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        async fn provides_best_route() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 10000),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].swaps[0].route.pools.len(), 1); // 0 -> 2 at 10:11
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![TOKEN0.clone().into(), TOKEN2.clone().into()]
            );
            assert_eq!(
                result[0].input_amount().unwrap(),
                currency_amount!(TOKEN0, 10032)
            );
            assert_eq!(
                result[0].output_amount().unwrap(),
                currency_amount!(TOKEN2, 10000)
            );
            assert_eq!(result[1].swaps[0].route.pools.len(), 2); // 0 -> 1 -> 2 at 12:12:10
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    TOKEN0.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN2.clone().into()
                ]
            );
            assert_eq!(
                result[1].input_amount().unwrap(),
                currency_amount!(TOKEN0, 15488)
            );
            assert_eq!(
                result[1].output_amount().unwrap(),
                currency_amount!(TOKEN2, 10000)
            );
        }

        #[tokio::test]
        async fn respects_max_hops() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 10),
                BestTradeOptions {
                    max_hops: Some(1),
                    ..Default::default()
                },
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].swaps[0].route.pools.len(), 1); // 0 -> 2 at 10:11
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![TOKEN0.clone().into(), TOKEN2.clone().into()]
            );
        }

        #[tokio::test]
        async fn insufficient_liquidity() {
            let result = &mut vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 120000),
                BestTradeOptions::default(),
                vec![],
                None,
                result,
            )
            .await
            .unwrap();
            assert_eq!(result.len(), 0);
        }

        #[tokio::test]
        async fn insufficient_liquidity_in_one_pool_but_not_the_other() {
            let result = &mut vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 105000),
                BestTradeOptions::default(),
                vec![],
                None,
                result,
            )
            .await
            .unwrap();
            assert_eq!(result.len(), 1);
        }

        #[tokio::test]
        async fn respects_max_num_results() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_2.clone(), POOL_1_2.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 10),
                BestTradeOptions {
                    max_num_results: Some(1),
                    ..Default::default()
                },
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 1);
        }

        #[tokio::test]
        async fn no_path() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![POOL_0_1.clone(), POOL_0_3.clone(), POOL_1_3.clone()],
                &TOKEN0.clone(),
                &currency_amount!(TOKEN2, 10),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 0);
        }

        #[tokio::test]
        async fn works_for_ether_currency_input() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![
                    POOL_ETH_0.clone(),
                    POOL_0_1.clone(),
                    POOL_0_3.clone(),
                    POOL_1_3.clone(),
                ],
                &ETHER.clone(),
                &currency_amount!(TOKEN3, 10000),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].input_currency().clone(), ETHER.clone());
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![
                    ETHER.clone().into(),
                    TOKEN0.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN3.clone().into()
                ]
            );
            assert_eq!(result[0].output_currency().clone(), TOKEN3.clone());
            assert_eq!(result[1].input_currency().clone(), ETHER.clone());
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    ETHER.clone().into(),
                    TOKEN0.clone().into(),
                    TOKEN3.clone().into()
                ]
            );
            assert_eq!(result[1].output_currency().clone(), TOKEN3.clone());
        }

        #[tokio::test]
        async fn works_for_ether_currency_output() {
            let mut result = vec![];
            Trade::best_trade_exact_out(
                vec![
                    POOL_ETH_0.clone(),
                    POOL_0_1.clone(),
                    POOL_0_3.clone(),
                    POOL_1_3.clone(),
                ],
                &TOKEN3.clone(),
                &currency_amount!(ETHER, 100),
                BestTradeOptions::default(),
                vec![],
                None,
                &mut result,
            )
            .await
            .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result[0].input_currency().clone(), TOKEN3.clone());
            assert_eq!(
                result[0].swaps[0].route.currency_path(),
                vec![
                    TOKEN3.clone().into(),
                    TOKEN0.clone().into(),
                    ETHER.clone().into()
                ]
            );
            assert_eq!(result[0].output_currency().clone(), ETHER.clone());
            assert_eq!(result[1].input_currency().clone(), TOKEN3.clone());
            assert_eq!(
                result[1].swaps[0].route.currency_path(),
                vec![
                    TOKEN3.clone().into(),
                    TOKEN1.clone().into(),
                    TOKEN0.clone().into(),
                    ETHER.clone().into()
                ]
            );
            assert_eq!(result[1].output_currency().clone(), ETHER.clone());
        }
    }
}
