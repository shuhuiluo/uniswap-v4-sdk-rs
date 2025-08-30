//! Common token definitions used across examples

use once_cell::sync::Lazy;
use uniswap_sdk_core::{prelude::*, token};

pub static ETHER: Lazy<Ether> = Lazy::new(|| Ether::on_chain(1));

pub static USDC: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        6,
        "USDC",
        "USD Coin"
    )
});

pub static DAI: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "6B175474E89094C44Da98b954EedeAC495271d0F",
        18,
        "DAI",
        "DAI Stablecoin"
    )
});

pub static WETH: Lazy<Token> = Lazy::new(|| ETHER.wrapped().clone());
