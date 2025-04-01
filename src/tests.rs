use crate::entities::Pool;
pub(crate) use alloc::vec;
use alloy::uint;
use once_cell::sync::Lazy;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;

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
pub(crate) static TOKEN0: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "0000000000000000000000000000000000000001",
        18,
        "t0",
        "token0"
    )
});
pub(crate) static TOKEN1: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "0000000000000000000000000000000000000002",
        18,
        "t1",
        "token1"
    )
});
pub(crate) static TOKEN2: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "0000000000000000000000000000000000000003",
        18,
        "t2",
        "token2"
    )
});
pub(crate) static TOKEN3: Lazy<Token> = Lazy::new(|| {
    token!(
        1,
        "0000000000000000000000000000000000000004",
        18,
        "t3",
        "token3"
    )
});

pub(crate) static USDC_DAI: Lazy<Pool> = Lazy::new(|| {
    Pool::new(
        USDC.clone().into(),
        DAI.clone().into(),
        FeeAmount::LOWEST.into(),
        uint!(0_U24),
        10,
        Address::ZERO,
        encode_sqrt_ratio_x96(1, 1),
        0,
    )
    .unwrap()
});
pub(crate) static DAI_USDC: Lazy<Pool> = Lazy::new(|| {
    Pool::new(
        DAI.clone().into(),
        USDC.clone().into(),
        FeeAmount::LOWEST.into(),
        uint!(0_U24),
        10,
        Address::ZERO,
        encode_sqrt_ratio_x96(1, 1),
        0,
    )
    .unwrap()
});

pub(crate) const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

pub(crate) static TICK_LIST: Lazy<Vec<Tick>> = Lazy::new(|| {
    vec![
        Tick {
            index: nearest_usable_tick(MIN_TICK_I32, 10),
            liquidity_net: ONE_ETHER as i128,
            liquidity_gross: ONE_ETHER,
        },
        Tick {
            index: nearest_usable_tick(MAX_TICK_I32, 10),
            liquidity_net: -(ONE_ETHER as i128),
            liquidity_gross: ONE_ETHER,
        },
    ]
});
