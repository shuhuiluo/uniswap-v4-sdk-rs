use crate::entities::Pool;
use once_cell::sync::Lazy;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::{constants::FeeAmount, prelude::encode_sqrt_ratio_x96};

pub(crate) static ETHER: Lazy<Ether> = Lazy::new(|| Ether::on_chain(1));
pub(crate) static WETH: Lazy<Token> = Lazy::new(|| ETHER.wrapped().clone());
pub(crate) static USDC: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        6,
        "USDC",
        "USD Coin"
    )
});
pub(crate) static DAI: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "6B175474E89094C44Da98b954EedeAC495271d0F",
        18,
        "DAI",
        "DAI Stablecoin"
    )
});

pub(crate) static USDC_DAI: Lazy<Pool> = Lazy::new(|| {
    Pool::new(
        Currency::Token(USDC.clone()),
        Currency::Token(DAI.clone()),
        FeeAmount::LOWEST.into(),
        10,
        Address::ZERO,
        encode_sqrt_ratio_x96(1, 1),
        0,
    )
    .unwrap()
});
pub(crate) static DAI_USDC: Lazy<Pool> = Lazy::new(|| {
    Pool::new(
        Currency::Token(DAI.clone()),
        Currency::Token(USDC.clone()),
        FeeAmount::LOWEST.into(),
        10,
        Address::ZERO,
        encode_sqrt_ratio_x96(1, 1),
        0,
    )
    .unwrap()
});

pub(crate) const ONE_ETHER: u128 = 1_000_000_000_000_000_000;
