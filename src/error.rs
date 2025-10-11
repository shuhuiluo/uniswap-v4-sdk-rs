#![allow(clippy::missing_inline_in_public_items)]

#[cfg(doc)]
use crate::prelude::*;

use alloy_sol_types::Error as SolError;
use uniswap_sdk_core::error::Error as CoreError;
use uniswap_v3_sdk::error::Error as V3Error;

#[derive(Debug, thiserror::Error)]
#[cfg_attr(not(feature = "extensions"), derive(Clone, PartialEq))]
pub enum Error {
    /// Thrown when an error occurs in the core library.
    #[error("{0}")]
    Core(#[from] CoreError),

    /// Thrown when an error occurs in the v3 library.
    #[error("{0}")]
    V3(#[from] V3Error),

    /// Thrown when an error occurs in the sol types library.
    #[error("{0}")]
    Sol(#[from] SolError),

    /// Thrown when the action is not supported.
    #[error("Unsupported action {0}")]
    InvalidAction(u8),

    /// Thrown when the currency passed to [`get_path_currency`] is not one of the pool's
    /// currencies.
    #[error("Invalid currency")]
    InvalidCurrency,

    /// Thrown when trying to simulate a swap with an unsupported hook.
    #[error("Unsupported hook")]
    UnsupportedHook,

    #[error("Insufficient liquidity")]
    InsufficientLiquidity,

    /// Thrown when a pool has been created but not yet initialized
    #[cfg(feature = "extensions")]
    #[error("Pool has been created but not yet initialized")]
    PoolNotInitialized,

    #[cfg(feature = "extensions")]
    #[error("{0}")]
    ContractError(#[from] alloy::contract::Error),
}

#[cfg(feature = "extensions")]
pub fn map_contract_error(e: Error) -> V3Error {
    match e {
        Error::ContractError(contract_error) => V3Error::ContractError(contract_error),
        _ => panic!("Unexpected error: {e:?}"),
    }
}
