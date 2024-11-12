use alloy_primitives::Address;
use uniswap_sdk_core::prelude::BaseCurrency;

#[inline]
pub fn to_address(currency: &impl BaseCurrency) -> Address {
    match currency.is_native() {
        true => Address::ZERO,
        false => currency.address(),
    }
}
