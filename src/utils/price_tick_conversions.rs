//! ## Price and tick conversions
//! Utility functions for converting between [`I24`] ticks and SDK Core [`Price`] prices.

use crate::prelude::{sorts_before, Error};
use alloy_primitives::{aliases::I24, U160};
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

/// Returns a price object corresponding to the input tick and the base/quote token.
/// Inputs must be tokens because the address order is used to interpret the price represented by
/// the tick.
///
/// ## Arguments
///
/// * `base_token`: the base token of the price
/// * `quote_token`: the quote token of the price
/// * `tick`: the tick for which to return the price
#[inline]
pub fn tick_to_price(
    base_currency: Currency,
    quote_currency: Currency,
    tick: I24,
) -> Result<Price<Currency, Currency>, Error> {
    let sqrt_ratio_x96 = get_sqrt_ratio_at_tick(tick)?;
    let ratio_x192 = sqrt_ratio_x96.to_big_int().pow(2);
    let q192 = Q192.to_big_int();
    Ok(if sorts_before(&base_currency, &quote_currency)? {
        Price::new(base_currency, quote_currency, q192, ratio_x192)
    } else {
        Price::new(base_currency, quote_currency, ratio_x192, q192)
    })
}

/// Returns the first tick for which the given price is greater than or equal to the tick price
///
/// ## Arguments
///
/// * `price`: for which to return the closest tick that represents a price less than or equal to
///   the input price, i.e. the price of the returned tick is less than or equal to the input price
#[inline]
pub fn price_to_closest_tick(price: &Price<Currency, Currency>) -> Result<I24, Error> {
    const ONE: I24 = I24::from_limbs([1]);
    let sorted = sorts_before(&price.base_currency, &price.quote_currency)?;
    let sqrt_ratio_x96: U160 = if sorted {
        encode_sqrt_ratio_x96(price.numerator, price.denominator)
    } else {
        encode_sqrt_ratio_x96(price.denominator, price.numerator)
    };
    let tick = sqrt_ratio_x96.get_tick_at_sqrt_ratio()?;
    let next_tick_price = tick_to_price(
        price.base_currency.clone(),
        price.quote_currency.clone(),
        tick + ONE,
    )?;
    Ok(if sorted {
        if price >= &next_tick_price {
            tick + ONE
        } else {
            tick
        }
    } else if price <= &next_tick_price {
        tick + ONE
    } else {
        tick
    })
}
