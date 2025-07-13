//! Test utilities and helper functions
#![cfg_attr(not(test), allow(dead_code))]

use crate::entities::Pool;
pub(crate) use alloc::vec;
use alloy_primitives::{address, Address, U160};
use once_cell::sync::Lazy;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;

pub const PERMIT2_ADDRESS: Address = address!("000000000022D473030F116dDEE9F6B43aC78BA3");

pub static ETHER: Lazy<Ether> = Lazy::new(|| Ether::on_chain(1));

pub(crate) static WETH: Lazy<Token> = Lazy::new(|| ETHER.wrapped().clone());

pub static USDC: Lazy<Token> = Lazy::new(|| {
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
        10,
        Address::ZERO,
        *SQRT_PRICE_1_1,
        0,
    )
    .unwrap()
});
pub(crate) static DAI_USDC: Lazy<Pool> = Lazy::new(|| {
    Pool::new(
        DAI.clone().into(),
        USDC.clone().into(),
        FeeAmount::LOWEST.into(),
        10,
        Address::ZERO,
        *SQRT_PRICE_1_1,
        0,
    )
    .unwrap()
});

pub(crate) const ONE_ETHER: u128 = 1_000_000_000_000_000_000;
pub(crate) static SQRT_PRICE_1_1: Lazy<U160> = Lazy::new(|| encode_sqrt_ratio_x96(1, 1));

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

#[macro_export]
macro_rules! currency_amount {
    ($currency:expr, $amount:expr) => {
        CurrencyAmount::from_raw_amount($currency.clone(), $amount).unwrap()
    };
}

#[macro_export]
macro_rules! create_route {
    ($pool:expr, $token_in:expr, $token_out:expr) => {
        $crate::entities::Route::new(vec![$pool.clone()], $token_in.clone(), $token_out.clone()).unwrap()
    };
    ($($pool:expr),+; $token_in:expr, $token_out:expr) => {
        $crate::entities::Route::new(vec![$($pool.clone()),+], $token_in.clone(), $token_out.clone()).unwrap()
    };
}

#[macro_export]
macro_rules! trade_from_route {
    ($route:expr, $amount:expr, $trade_type:expr) => {
        $crate::entities::Trade::from_route($route.clone(), $amount.clone(), $trade_type)
            .await
            .unwrap()
    };
}

#[cfg(feature = "extensions")]
pub(crate) use extensions::*;

#[cfg(feature = "extensions")]
mod extensions {
    use super::*;
    use crate::abi::IStateView;
    use alloy::{
        eips::{BlockId, BlockNumberOrTag},
        providers::{ProviderBuilder, RootProvider},
        transports::http::reqwest::Url,
    };

    pub(crate) static RPC_URL: Lazy<Url> = Lazy::new(|| {
        dotenv::dotenv().ok();
        std::env::var("MAINNET_RPC_URL").unwrap().parse().unwrap()
    });

    pub(crate) static PROVIDER: Lazy<RootProvider> = Lazy::new(|| {
        ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(RPC_URL.clone())
    });

    pub(crate) const BLOCK_ID: Option<BlockId> =
        Some(BlockId::Number(BlockNumberOrTag::Number(22305544)));

    pub(crate) static POOL_ID_ETH_USDC: Lazy<B256> = Lazy::new(|| {
        Pool::get_pool_id(
            &ETHER.clone().into(),
            &USDC.clone().into(),
            FeeAmount::LOW.into(),
            10,
            Address::ZERO,
        )
        .unwrap()
    });

    pub(crate) static STATE_VIEW: Lazy<IStateView::IStateViewInstance<RootProvider>> =
        Lazy::new(|| {
            IStateView::new(
                CHAIN_TO_ADDRESSES_MAP
                    .get(&1)
                    .unwrap()
                    .v4_state_view
                    .unwrap(),
                PROVIDER.clone(),
            )
        });
}

#[cfg(all(feature = "extensions", feature = "test-utils"))]
pub use examples::*;

#[cfg(all(feature = "extensions", feature = "test-utils"))]
mod examples {
    use super::*;
    use crate::{
        position_manager::{AddLiquidityOptions, AddLiquiditySpecificOptions, MintSpecificOptions},
        prelude::*,
    };
    use alloc::boxed::Box;
    use alloy::{
        providers::{
            ext::AnvilApi,
            fillers::{
                BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            },
            layers::AnvilProvider,
            Identity, Provider, ProviderBuilder, RootProvider,
        },
        signers::{local::PrivateKeySigner, SignerSync},
    };
    use alloy_primitives::{aliases::U24, Bytes, Signature, B256, U256};
    use alloy_sol_types::{eip712_domain, Eip712Domain, SolStruct};

    /// Set up an Anvil fork from mainnet at a specific block
    #[inline]
    pub async fn setup_anvil_fork(
        fork_block: u64,
    ) -> FillProvider<
        JoinFill<
            Identity,
            JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
        >,
        AnvilProvider<RootProvider>,
    > {
        ProviderBuilder::new().connect_anvil_with_config(|anvil| {
            anvil.fork(RPC_URL.clone()).fork_block_number(fork_block)
        })
    }

    /// Create a test account with ETH balance
    #[inline]
    pub async fn setup_test_account(provider: &impl Provider, balance: U256) -> PrivateKeySigner {
        let signer = PrivateKeySigner::random();
        let account = signer.address();
        provider.anvil_set_balance(account, balance).await.unwrap();
        signer
    }

    /// Create a pool from on-chain state
    #[inline]
    pub async fn create_pool(
        provider: &impl Provider,
        v4_pool_manager: Address,
        currency0: Currency,
        currency1: Currency,
        fee: U24,
        tick_spacing: i32,
        hook_address: Address,
    ) -> Result<Pool, Box<dyn core::error::Error>> {
        let pool_id = Pool::get_pool_id(&currency0, &currency1, fee, tick_spacing, hook_address)?;

        let pool_lens = PoolManagerLens::new(v4_pool_manager, provider);
        let (actual_sqrt_price, _, _, _) = pool_lens.get_slot0(pool_id, None).await?;

        Ok(Pool::new(
            currency0,
            currency1,
            fee,
            tick_spacing,
            hook_address,
            actual_sqrt_price,
            0,
        )?)
    }

    /// Setup token balance and approval for a single token
    #[inline]
    pub async fn setup_token_balance(
        provider: &impl Provider,
        token_address: Address,
        account: Address,
        amount: U256,
        approve_to: Address,
    ) -> Result<(), Box<dyn core::error::Error>> {
        let overrides =
            get_erc20_state_overrides(token_address, account, approve_to, amount, provider).await?;

        for (token, account_override) in overrides {
            for (slot, value) in account_override.state_diff.unwrap() {
                provider
                    .anvil_set_storage_at(token, U256::from_be_bytes(slot.0), value)
                    .await?;
            }
        }

        Ok(())
    }

    /// Get Permit2 EIP-712 domain for mainnet
    #[inline]
    #[must_use]
    pub const fn get_permit2_domain() -> Eip712Domain {
        eip712_domain! {
            name: "Permit2",
            chain_id: 1,
            verifying_contract: PERMIT2_ADDRESS,
        }
    }

    /// Create EIP-712 signature for Permit2 batch permit
    #[inline]
    pub fn create_permit2_signature(
        permit_batch: &AllowanceTransferPermitBatch,
        signer: &PrivateKeySigner,
    ) -> Result<Bytes, Box<dyn core::error::Error>> {
        let domain = get_permit2_domain();
        let hash: B256 = permit_batch.eip712_signing_hash(&domain);
        let signature: Signature = signer.sign_hash_sync(&hash)?;
        Ok(signature.as_bytes().into())
    }

    /// Create AddLiquidityOptions for minting positions
    #[inline]
    pub fn create_add_liquidity_options(
        recipient: Address,
        batch_permit: Option<BatchPermitOptions>,
    ) -> AddLiquidityOptions {
        AddLiquidityOptions {
            common_opts: CommonOptions {
                slippage_tolerance: Percent::new(1, 1000),
                deadline: U256::MAX,
                hook_data: Default::default(),
            },
            use_native: Some(ETHER.clone()),
            batch_permit,
            specific_opts: AddLiquiditySpecificOptions::Mint(MintSpecificOptions {
                recipient,
                create_pool: false,
                sqrt_price_x96: None,
                migrate: false,
            }),
        }
    }
}
