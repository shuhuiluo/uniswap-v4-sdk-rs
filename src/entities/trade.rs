use crate::prelude::{amount_with_path_currency, Error, Pool, Route};
use rustc_hash::FxHashSet;
use uniswap_sdk_core::prelude::{sorted_insert::sorted_insert, *};
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
    pub fn input_currency(&self) -> &TInput {
        &self.input_amount.currency
    }

    /// Returns the output currency of the swap
    #[inline]
    pub fn output_currency(&self) -> &TOutput {
        &self.output_amount.currency
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
        let pool_id_set = FxHashSet::from_iter(pool_ids);
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
    pub fn exact_in(
        route: Route<TInput, TOutput, TP>,
        amount_in: CurrencyAmount<impl BaseCurrency>,
    ) -> Result<Self, Error> {
        Self::from_route(route, amount_in, TradeType::ExactInput)
    }

    /// Constructs an exact out trade with the given amount out and route
    ///
    /// ## Arguments
    ///
    /// * `route`: The route of the exact out trade
    /// * `amount_out`: The amount returned by the trade
    #[inline]
    pub fn exact_out(
        route: Route<TInput, TOutput, TP>,
        amount_out: CurrencyAmount<impl BaseCurrency>,
    ) -> Result<Self, Error> {
        Self::from_route(route, amount_out, TradeType::ExactOutput)
    }

    /// Constructs a trade by simulating swaps through the given route
    ///
    /// ## Arguments
    ///
    /// * `route`: The route to swap through
    /// * `amount`: The amount specified, either input or output, depending on `trade_type`
    /// * `trade_type`: Whether the trade is an exact input or exact output swap
    #[inline]
    pub fn from_route(
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
                    (token_amount, _) = pool.get_output_amount(&token_amount, None)?;
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
                    amount_with_path_currency(&amount, &route.pools.last().unwrap())?;
                for pool in route.pools.iter().rev() {
                    (token_amount, _) = pool.get_input_amount(&token_amount, None)?;
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
    pub fn from_routes(
        routes: Vec<(
            CurrencyAmount<impl BaseCurrency>,
            Route<TInput, TOutput, TP>,
        )>,
        trade_type: TradeType,
    ) -> Result<Self, Error> {
        let mut populated_routes: Vec<Swap<TInput, TOutput, TP>> = Vec::with_capacity(routes.len());
        for (amount, route) in routes {
            let trade = Self::from_route(route, amount, trade_type)?;
            populated_routes.push(trade.swaps[0].clone());
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
    pub fn best_trade_exact_in<'a>(
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
                Some(amount_in) => pool.get_output_amount(amount_in, None),
                None => pool.get_output_amount(currency_amount_in, None),
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
                )?;
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
                Self::best_trade_exact_in(
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
                )?;
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
    pub fn best_trade_exact_out<'a>(
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
                Some(amount_out) => pool.get_input_amount(amount_out, None),
                None => pool.get_input_amount(currency_amount_out, None),
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
                )?;
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
                Self::best_trade_exact_out(
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
                )?;
            }
        }
        Ok(best_trades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use once_cell::sync::Lazy;

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
            CurrencyAmount::from_raw_amount(TOKEN0.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN1.clone().into(), 100000).unwrap(),
            None,
        )
    });
    static POOL_0_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(TOKEN0.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN2.clone().into(), 110000).unwrap(),
            None,
        )
    });
    static POOL_0_3: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(TOKEN0.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN3.clone().into(), 90000).unwrap(),
            None,
        )
    });
    static POOL_1_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(TOKEN1.clone().into(), 120000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN2.clone().into(), 100000).unwrap(),
            None,
        )
    });
    static POOL_1_3: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(TOKEN1.clone().into(), 120000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN3.clone().into(), 130000).unwrap(),
            None,
        )
    });
    static POOL_ETH_0: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(ETHER.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN0.clone().into(), 100000).unwrap(),
            None,
        )
    });
    static POOL_ETH_1: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(ETHER.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN1.clone().into(), 100000).unwrap(),
            None,
        )
    });
    static POOL_ETH_2: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(ETHER.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN2.clone().into(), 100000).unwrap(),
            None,
        )
    });
    static POOL_WETH_0: Lazy<Pool<TickListDataProvider>> = Lazy::new(|| {
        v2_style_pool(
            CurrencyAmount::from_raw_amount(WETH.clone().into(), 100000).unwrap(),
            CurrencyAmount::from_raw_amount(TOKEN0.clone().into(), 100000).unwrap(),
            None,
        )
    });

    mod from_route {
        use super::*;

        #[test]
        fn can_be_constructed_with_ether_as_input() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], ETHER.clone(), TOKEN0.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(ETHER.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_input_on_a_weth_pool() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_WETH_0.clone()], ETHER.clone(), TOKEN0.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(ETHER.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_weth_as_input_on_a_eth_pool() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], WETH.clone(), TOKEN0.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(WETH.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), WETH.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output_on_a_weth_pool() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_WETH_0.clone()], TOKEN0.clone(), ETHER.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(TOKEN0.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[test]
        fn can_be_constructed_with_weth_as_output_on_a_eth_pool() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], TOKEN0.clone(), WETH.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(TOKEN0.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), WETH.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_input_for_exact_output() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], ETHER.clone(), TOKEN0.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(TOKEN0.clone(), 10000).unwrap(),
                TradeType::ExactOutput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], TOKEN0.clone(), ETHER.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(ETHER.clone(), 10000).unwrap(),
                TradeType::ExactOutput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output_for_exact_input() {
            let trade = Trade::from_route(
                Route::new(vec![POOL_ETH_0.clone()], TOKEN0.clone(), ETHER.clone()).unwrap(),
                CurrencyAmount::from_raw_amount(TOKEN0.clone(), 10000).unwrap(),
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }
    }

    mod from_routes {
        use super::*;

        #[test]
        fn can_be_constructed_with_ether_as_input_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![(
                    CurrencyAmount::from_raw_amount(ETHER.clone(), 10000).unwrap(),
                    Route::new(vec![POOL_ETH_0.clone()], ETHER.clone(), TOKEN0.clone()).unwrap(),
                )],
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_input_for_exact_output_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 3000).unwrap(),
                        Route::new(vec![POOL_ETH_0.clone()], ETHER.clone(), TOKEN0.clone())
                            .unwrap(),
                    ),
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 7000).unwrap(),
                        Route::new(
                            vec![POOL_ETH_1.clone(), POOL_0_1.clone()],
                            ETHER.clone(),
                            TOKEN0.clone(),
                        )
                        .unwrap(),
                    ),
                ],
                TradeType::ExactOutput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), ETHER.clone());
            assert_eq!(trade.output_currency().clone(), TOKEN0.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        CurrencyAmount::from_raw_amount(ETHER.clone(), 4000).unwrap(),
                        Route::new(vec![POOL_ETH_0.clone()], TOKEN0.clone(), ETHER.clone())
                            .unwrap(),
                    ),
                    (
                        CurrencyAmount::from_raw_amount(ETHER.clone(), 6000).unwrap(),
                        Route::new(
                            vec![POOL_0_1.clone(), POOL_ETH_1.clone()],
                            TOKEN0.clone(),
                            ETHER.clone(),
                        )
                        .unwrap(),
                    ),
                ],
                TradeType::ExactOutput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[test]
        fn can_be_constructed_with_ether_as_output_for_exact_input_with_multiple_routes() {
            let trade = Trade::from_routes(
                vec![
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 3000).unwrap(),
                        Route::new(vec![POOL_ETH_0.clone()], TOKEN0.clone(), ETHER.clone())
                            .unwrap(),
                    ),
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 7000).unwrap(),
                        Route::new(
                            vec![POOL_0_1.clone(), POOL_ETH_1.clone()],
                            TOKEN0.clone(),
                            ETHER.clone(),
                        )
                        .unwrap(),
                    ),
                ],
                TradeType::ExactInput,
            )
            .unwrap();
            assert_eq!(trade.input_currency().clone(), TOKEN0.clone());
            assert_eq!(trade.output_currency().clone(), ETHER.clone());
        }

        #[test]
        #[should_panic(expected = "POOLS_DUPLICATED")]
        fn throws_if_pools_are_reused_between_routes() {
            let _ = Trade::from_routes(
                vec![
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 4500).unwrap(),
                        Route::new(
                            vec![POOL_0_1.clone(), POOL_ETH_1.clone()],
                            TOKEN0.clone(),
                            ETHER.clone(),
                        )
                        .unwrap(),
                    ),
                    (
                        CurrencyAmount::from_raw_amount(TOKEN0.clone(), 5500).unwrap(),
                        Route::new(
                            vec![POOL_0_1.clone(), POOL_1_2.clone(), POOL_ETH_2.clone()],
                            TOKEN0.clone(),
                            ETHER.clone(),
                        )
                        .unwrap(),
                    ),
                ],
                TradeType::ExactInput,
            );
        }
    }
}
