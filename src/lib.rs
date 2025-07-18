#![doc = include_str!("../README.md")]
#![cfg_attr(not(any(feature = "std", all(test, feature = "extensions"))), no_std)]
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    unreachable_pub,
    clippy::missing_const_for_fn,
    clippy::missing_inline_in_public_items,
    clippy::needless_pass_by_value,
    clippy::redundant_clone,
    clippy::explicit_iter_loop,
    clippy::manual_assert,
    clippy::must_use_candidate,
    clippy::semicolon_if_nothing_returned,
    clippy::suspicious_operation_groupings,
    clippy::unseparated_literal_suffix,
    clippy::unused_self,
    clippy::use_debug,
    clippy::use_self,
    rustdoc::all
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(test, allow(dead_code))]

extern crate alloc;

pub mod abi;
pub mod entities;
pub mod error;
pub mod position_manager;
pub mod utils;

pub use uniswap_v3_sdk::multicall;

#[cfg(feature = "extensions")]
pub mod extensions;

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;

pub mod prelude {
    pub use crate::{abi::*, entities::*, error::*, multicall::*, position_manager::*, utils::*};

    pub use uniswap_sdk_core as sdk_core;
    pub use uniswap_v3_sdk as v3_sdk;

    #[cfg(feature = "extensions")]
    pub use crate::extensions::*;
}
