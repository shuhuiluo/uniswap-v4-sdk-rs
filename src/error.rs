#[cfg(doc)]
use crate::prelude::*;

use alloy_sol_types::Error as SolError;
use derive_more::From;
use uniswap_sdk_core::error::Error as CoreError;
use uniswap_v3_sdk::error::Error as V3Error;

#[derive(Debug, From)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum Error {
    /// Thrown when an error occurs in the core library.
    #[cfg_attr(feature = "std", error("{0}"))]
    Core(#[cfg_attr(not(feature = "std"), from)] CoreError),

    /// Thrown when an error occurs in the v3 library.
    #[cfg_attr(feature = "std", error("{0}"))]
    V3(#[cfg_attr(not(feature = "std"), from)] V3Error),

    /// Thrown when an error occurs in the sol types library.
    #[cfg_attr(feature = "std", error("{0}"))]
    Sol(#[cfg_attr(not(feature = "std"), from)] SolError),

    /// Thrown when the action is not supported.
    #[cfg_attr(feature = "std", error("Unsupported action {0}"))]
    InvalidAction(u8),

    /// Thrown when the currency passed to [`get_path_currency`] is not one of the pool's
    /// currencies.
    #[cfg_attr(feature = "std", error("Invalid currency"))]
    InvalidCurrency,

    /// Thrown when trying to simulate a swap with an unsupported hook.
    #[cfg_attr(feature = "std", error("Unsupported hook"))]
    UnsupportedHook,

    #[cfg_attr(feature = "std", error("Insufficient liquidity"))]
    InsufficientLiquidity,
}
