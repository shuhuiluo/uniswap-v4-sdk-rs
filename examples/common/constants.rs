//! Common constants used across examples

use alloy::{eips::BlockId, transports::http::reqwest::Url};
use alloy_primitives::{address, Address};
use once_cell::sync::Lazy;
use uniswap_sdk_core::addresses::CHAIN_TO_ADDRESSES_MAP;

pub static RPC_URL: Lazy<Url> = Lazy::new(|| {
    dotenv::dotenv().ok();
    std::env::var("MAINNET_RPC_URL").unwrap().parse().unwrap()
});

pub const BLOCK_ID: Option<BlockId> = Some(BlockId::number(22305544));

pub const PERMIT2_ADDRESS: Address = address!("000000000022D473030F116dDEE9F6B43aC78BA3");

static V4_ADDRESSES: Lazy<&uniswap_sdk_core::addresses::ChainAddresses> =
    Lazy::new(|| CHAIN_TO_ADDRESSES_MAP.get(&1).unwrap());

pub static V4_POSITION_MANAGER: Lazy<Address> =
    Lazy::new(|| V4_ADDRESSES.v4_position_manager.unwrap());

pub static V4_POOL_MANAGER: Lazy<Address> = Lazy::new(|| V4_ADDRESSES.v4_pool_manager.unwrap());
