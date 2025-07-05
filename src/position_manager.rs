use crate::prelude::{Error, *};
use alloc::vec::Vec;
use alloy_primitives::{address, Address, Bytes, Signature, U160, U256};
use alloy_sol_types::{eip712_domain, SolCall};
use derive_more::{Deref, DerefMut, From};
use num_traits::ToPrimitive;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::{
    IERC721Permit, MethodParameters, MintAmounts, TickDataProvider, TickIndex,
};

pub use uniswap_v3_sdk::prelude::NFTPermitData;

/// Shared Action Constants used in the v4 Router and v4 position manager
pub const MSG_SENDER: Address = address!("0000000000000000000000000000000000000001");

/// Used when unwrapping weth in positon manager
pub const OPEN_DELTA: U256 = U256::ZERO;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommonOptions {
    /// How much the pool price is allowed to move from the specified action.
    pub slippage_tolerance: Percent,
    /// When the transaction expires, in epoch seconds.
    pub deadline: U256,
    /// Optional data to pass to hooks.
    pub hook_data: Bytes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifyPositionSpecificOptions {
    /// Indicates the ID of the position to increase liquidity for.
    pub token_id: U256,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintSpecificOptions {
    /// The account that should receive the minted NFT.
    pub recipient: Address,
    /// Creates pool if not initialized before mint.
    pub create_pool: bool,
    /// Initial price to set on the pool if creating.
    pub sqrt_price_x96: Option<U160>,
    /// Whether the mint is part of a migration from V3 to V4.
    pub migrate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, From)]
pub enum AddLiquiditySpecificOptions {
    Mint(#[from] MintSpecificOptions),
    Increase(#[from] ModifyPositionSpecificOptions),
}

/// Options for producing the calldata to add liquidity.
#[derive(Clone, Debug, PartialEq, Deref, DerefMut)]
pub struct AddLiquidityOptions {
    #[deref]
    #[deref_mut]
    pub common_opts: CommonOptions,
    /// Whether to spend ether. If true, one of the currencies must be the NATIVE currency.
    pub use_native: Option<Ether>,
    /// The optional permit2 batch permit parameters for spending token0 and token1.
    pub batch_permit: Option<BatchPermitOptions>,
    /// [`MintSpecificOptions`] or [`IncreaseSpecificOptions`]
    pub specific_opts: AddLiquiditySpecificOptions,
}

impl Default for AddLiquidityOptions {
    #[inline]
    fn default() -> Self {
        Self {
            common_opts: Default::default(),
            use_native: None,
            batch_permit: None,
            specific_opts: MintSpecificOptions::default().into(),
        }
    }
}

/// Options for producing the calldata to exit a position.
#[derive(Debug, Clone, PartialEq, Eq, Deref, DerefMut)]
pub struct RemoveLiquidityOptions {
    #[deref]
    #[deref_mut]
    pub common_opts: CommonOptions,
    /// The ID of the token to exit
    pub token_id: U256,
    /// The percentage of position liquidity to exit.
    pub liquidity_percentage: Percent,
    /// Whether the NFT should be burned if the entire position is being exited, by default false.
    pub burn_token: bool,
    /// The optional permit of the token ID being exited, in case the exit transaction is being
    /// sent by an account that does not own the NFT
    pub permit: Option<NFTPermitOptions>,
}

impl Default for RemoveLiquidityOptions {
    #[inline]
    fn default() -> Self {
        Self {
            common_opts: Default::default(),
            token_id: U256::ZERO,
            liquidity_percentage: Percent::new(1, 1),
            burn_token: false,
            permit: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deref, DerefMut)]
pub struct CollectOptions {
    #[deref]
    #[deref_mut]
    pub common_opts: CommonOptions,
    /// Indicates the ID of the position to collect for.
    pub token_id: U256,
    /// The account that should receive the tokens.
    pub recipient: Address,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TransferOptions {
    /// The account sending the NFT.
    pub sender: Address,
    /// The account that should receive the NFT.
    pub recipient: Address,
    /// The id of the token being sent.
    pub token_id: U256,
}

pub type AllowanceTransferPermitSingle = IAllowanceTransfer::PermitSingle;
pub type AllowanceTransferPermitBatch = IAllowanceTransfer::PermitBatch;
pub type NFTPermitValues = IERC721Permit::Permit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPermitOptions {
    pub owner: Address,
    pub permit_batch: AllowanceTransferPermitBatch,
    pub signature: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Deref, DerefMut)]
pub struct NFTPermitOptions {
    #[deref]
    #[deref_mut]
    pub values: NFTPermitValues,
    pub signature: Signature,
}

/// Public methods to encode method parameters for different actions on the PositionManager contract
#[inline]
#[must_use]
pub fn create_call_parameters(pool_key: PoolKey, sqrt_price_x96: U160) -> MethodParameters {
    MethodParameters {
        calldata: encode_initialize_pool(pool_key, sqrt_price_x96),
        value: U256::ZERO,
    }
}

/// Encodes the method parameters for adding liquidity to a position.
///
/// ## Notes
///
/// - If the pool does not exist yet, the `initializePool` call is encoded.
/// - If it is a mint, encode `MINT_POSITION`. If migrating, encode a `SETTLE` and `SWEEP` for both
///   currencies. Else, encode a `SETTLE_PAIR`. If on a NATIVE pool, encode a `SWEEP`.
/// - Else, encode `INCREASE_LIQUIDITY` and `SETTLE_PAIR`. If it is on a NATIVE pool, encode a
///   `SWEEP`.
///
/// ## Arguments
///
/// * `position`: The position to be added.
/// * `options`: The options for adding liquidity.
#[inline]
pub fn add_call_parameters<TP: TickDataProvider>(
    position: &mut Position<TP>,
    options: AddLiquidityOptions,
) -> Result<MethodParameters, Error> {
    assert!(position.liquidity > 0, "ZERO_LIQUIDITY");

    let mut calldatas: Vec<Bytes> = Vec::with_capacity(3);
    let mut planner = V4PositionPlanner::default();

    // Encode initialize pool.
    if let AddLiquiditySpecificOptions::Mint(opts) = options.specific_opts {
        if opts.create_pool {
            // No planner used here because initializePool is not supported as an Action
            calldatas.push(encode_initialize_pool(
                position.pool.pool_key.clone(),
                opts.sqrt_price_x96.expect("NO_SQRT_PRICE"),
            ));
        }
    }

    // position.pool.currency0 is native if and only if options.useNative is set
    assert!(
        if let Some(ether) = &options.use_native {
            position.pool.currency0.equals(ether)
        } else {
            !position.pool.currency0.is_native()
        },
        "NATIVE_NOT_SET"
    );

    // adjust for slippage
    let MintAmounts {
        amount0: amount0_max,
        amount1: amount1_max,
    } = position.mint_amounts_with_slippage(&options.slippage_tolerance)?;

    // We use permit2 to approve tokens to the position manager
    if let Some(batch_permit) = options.batch_permit {
        calldatas.push(encode_permit_batch(
            batch_permit.owner,
            batch_permit.permit_batch,
            batch_permit.signature,
        ));
    }

    match options.specific_opts {
        AddLiquiditySpecificOptions::Mint(opts) => {
            planner.add_mint(
                &position.pool,
                position.tick_lower,
                position.tick_upper,
                U256::from(position.liquidity),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                opts.recipient,
                options.common_opts.hook_data,
            );
        }
        AddLiquiditySpecificOptions::Increase(opts) => {
            planner.add_increase(
                opts.token_id,
                U256::from(position.liquidity),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                options.common_opts.hook_data,
            );
        }
    }

    let mut value = U256::ZERO;

    // If migrating, we need to settle and sweep both currencies individually
    match options.specific_opts {
        AddLiquiditySpecificOptions::Mint(opts) if opts.migrate => {
            if options.use_native.is_some() {
                // unwrap the exact amount needed to send to the pool manager
                planner.add_unwrap(OPEN_DELTA);
                // payer is v4 position manager
                planner.add_settle(&position.pool.currency0, false, None);
                planner.add_settle(&position.pool.currency1, false, None);
                // sweep any leftover wrapped native that was not unwrapped
                // recipient will be the same as the v4 lp token recipient
                planner.add_sweep(position.pool.currency0.wrapped(), opts.recipient);
                planner.add_sweep(&position.pool.currency1, opts.recipient);
            } else {
                // payer is v4 position manager
                planner.add_settle(&position.pool.currency0, false, None);
                planner.add_settle(&position.pool.currency1, false, None);
                // recipient will be the same as the v4 lp token recipient
                planner.add_sweep(&position.pool.currency0, opts.recipient);
                planner.add_sweep(&position.pool.currency1, opts.recipient);
            }
        }
        _ => {
            // need to settle both currencies when minting / adding liquidity (user is the payer)
            planner.add_settle_pair(&position.pool.currency0, &position.pool.currency1);
            // When not migrating and adding native currency, add a final sweep
            if options.use_native.is_some() {
                // Any sweeping must happen after the settling.
                // native currency will always be currency0 in v4
                value = amount0_max;
                planner.add_sweep(&position.pool.currency0, MSG_SENDER);
            }
        }
    }

    calldatas.push(encode_modify_liquidities(
        planner.0.finalize(),
        options.common_opts.deadline,
    ));

    Ok(MethodParameters {
        calldata: encode_multicall(calldatas),
        value,
    })
}

/// Produces the calldata for completely or partially exiting a position
///
/// ## Notes
///
/// - If the liquidity percentage is 100%, encode `BURN_POSITION` and then `TAKE_PAIR`.
/// - Else, encode `DECREASE_LIQUIDITY` and then `TAKE_PAIR`.
///
/// ## Arguments
///
/// * `position`: The position to exit
/// * `options`: Additional information necessary for generating the calldata
#[inline]
pub fn remove_call_parameters<TP: TickDataProvider>(
    position: &Position<TP>,
    options: RemoveLiquidityOptions,
) -> Result<MethodParameters, Error> {
    let mut calldatas: Vec<Bytes> = Vec::with_capacity(2);
    let mut planner = V4PositionPlanner::default();

    let token_id = options.token_id;

    if options.burn_token {
        // if burnToken is true, the specified liquidity percentage must be 100%
        assert_eq!(
            options.liquidity_percentage,
            Percent::new(1, 1),
            "CANNOT_BURN"
        );

        // if there is a permit, encode the ERC721Permit permit call
        if let Some(permit) = options.permit {
            calldatas.push(encode_erc721_permit(
                permit.spender,
                token_id,
                permit.deadline,
                permit.nonce,
                permit.signature.as_bytes().into(),
            ));
        }

        // slippage-adjusted amounts derived from current position liquidity
        let (amount0_min, amount1_min) =
            position.burn_amounts_with_slippage(&options.common_opts.slippage_tolerance)?;
        planner.add_burn(
            token_id,
            u128::try_from(amount0_min).unwrap(),
            u128::try_from(amount1_min).unwrap(),
            options.common_opts.hook_data,
        );
    } else {
        // construct a partial position with a percentage of liquidity
        let partial_position = Position::new(
            Pool::new(
                position.pool.currency0.clone(),
                position.pool.currency1.clone(),
                position.pool.fee,
                position.pool.tick_spacing.to_i24().as_i32(),
                position.pool.hooks,
                position.pool.sqrt_price_x96,
                position.pool.liquidity,
            )?,
            (options.liquidity_percentage * Percent::new(position.liquidity, 1))
                .quotient()
                .to_u128()
                .unwrap(),
            position.tick_lower.try_into().unwrap(),
            position.tick_upper.try_into().unwrap(),
        );

        // If the partial position has liquidity=0, this is a collect call and collectCallParameters
        // should be used
        assert!(partial_position.liquidity > 0, "ZERO_LIQUIDITY");

        // slippage-adjusted underlying amounts
        let (amount0_min, amount1_min) =
            partial_position.burn_amounts_with_slippage(&options.common_opts.slippage_tolerance)?;

        planner.add_decrease(
            token_id,
            U256::from(partial_position.liquidity),
            u128::try_from(amount0_min).unwrap(),
            u128::try_from(amount1_min).unwrap(),
            options.common_opts.hook_data,
        );
    }

    planner.add_take_pair(
        &position.pool.currency0,
        &position.pool.currency1,
        MSG_SENDER,
    );
    calldatas.push(encode_modify_liquidities(
        planner.0.finalize(),
        options.common_opts.deadline,
    ));

    Ok(MethodParameters {
        calldata: encode_multicall(calldatas),
        value: U256::ZERO,
    })
}

/// Produces the calldata for collecting fees from a position
///
/// ## Arguments
///
/// * `position`: The position to collect fees from
/// * `options`: Additional information necessary for generating the calldata
#[inline]
pub fn collect_call_parameters<TP: TickDataProvider>(
    position: &Position<TP>,
    options: CollectOptions,
) -> MethodParameters {
    let mut planner = V4PositionPlanner::default();

    // To collect fees in V4, we need to:
    // - encode a decrease liquidity by 0
    // - and encode a TAKE_PAIR
    planner.add_decrease(
        options.token_id,
        U256::ZERO,
        0,
        0,
        options.common_opts.hook_data,
    );

    planner.add_take_pair(
        &position.pool.currency0,
        &position.pool.currency1,
        options.recipient,
    );

    MethodParameters {
        calldata: encode_modify_liquidities(planner.0.finalize(), options.common_opts.deadline),
        value: U256::ZERO,
    }
}

#[inline]
fn encode_initialize_pool(pool_key: PoolKey, sqrt_price_x96: U160) -> Bytes {
    IPositionManager::initializePoolCall {
        key: pool_key,
        sqrtPriceX96: sqrt_price_x96,
    }
    .abi_encode()
    .into()
}

#[inline]
pub fn encode_modify_liquidities(unlock_data: Bytes, deadline: U256) -> Bytes {
    IPositionManager::modifyLiquiditiesCall {
        unlockData: unlock_data,
        deadline,
    }
    .abi_encode()
    .into()
}

#[inline]
pub fn encode_permit_batch(
    owner: Address,
    permit_batch: AllowanceTransferPermitBatch,
    signature: Bytes,
) -> Bytes {
    IPositionManager::permitBatchCall {
        owner,
        _permitBatch: permit_batch,
        signature,
    }
    .abi_encode()
    .into()
}

#[inline]
pub fn encode_erc721_permit(
    spender: Address,
    token_id: U256,
    deadline: U256,
    nonce: U256,
    signature: Bytes,
) -> Bytes {
    IPositionManager::permitCall {
        spender,
        tokenId: token_id,
        deadline,
        nonce,
        signature,
    }
    .abi_encode()
    .into()
}

/// Prepares the parameters for EIP712 signing
///
/// ## Arguments
///
/// * `permit`: The permit values to sign
/// * `position_manager`: The address of the position manager contract
/// * `chain_id`: The chain ID
///
/// ## Returns
///
/// The EIP712 domain and values to sign
///
/// ## Examples
///
/// ```
/// use alloy::signers::{local::PrivateKeySigner, SignerSync};
/// use alloy_primitives::{address, b256, uint, Signature, B256};
/// use alloy_sol_types::SolStruct;
/// use uniswap_v4_sdk::prelude::*;
///
/// let permit = NFTPermitValues {
///     spender: address!("000000000000000000000000000000000000000b"),
///     tokenId: uint!(1_U256),
///     nonce: uint!(1_U256),
///     deadline: uint!(123_U256),
/// };
/// assert_eq!(
///     permit.eip712_type_hash(),
///     b256!("49ecf333e5b8c95c40fdafc95c1ad136e8914a8fb55e9dc8bb01eaa83a2df9ad")
/// );
/// let data: NFTPermitData = get_permit_data(
///     permit,
///     address!("000000000000000000000000000000000000000b"),
///     1,
/// );
///
/// // Derive the EIP-712 signing hash.
/// let hash: B256 = data.eip712_signing_hash();
///
/// let signer = PrivateKeySigner::random();
/// let signature: Signature = signer.sign_hash_sync(&hash).unwrap();
/// assert_eq!(
///     signature.recover_address_from_prehash(&hash).unwrap(),
///     signer.address()
/// );
/// ```
#[inline]
#[must_use]
pub const fn get_permit_data(
    permit: NFTPermitValues,
    position_manager: Address,
    chain_id: u64,
) -> NFTPermitData {
    let domain = eip712_domain! {
        name: "Uniswap V4 Positions NFT",
        chain_id: chain_id,
        verifying_contract: position_manager,
    };
    NFTPermitData {
        domain,
        values: permit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use alloy_primitives::{address, hex, uint, Address, Bytes, Signature, U256};
    use once_cell::sync::Lazy;
    use uniswap_sdk_core::token;
    use uniswap_v3_sdk::prelude::{decode_multicall, FeeAmount};

    static CURRENCY0: Lazy<Currency> = Lazy::new(|| {
        token!(
            1,
            "0000000000000000000000000000000000000001",
            18,
            "t0",
            "currency0"
        )
        .into()
    });
    static CURRENCY1: Lazy<Currency> = Lazy::new(|| {
        token!(
            1,
            "0000000000000000000000000000000000000002",
            18,
            "t1",
            "currency1"
        )
        .into()
    });

    const FEE: FeeAmount = FeeAmount::MEDIUM;
    const TICK_SPACING: i32 = 60;

    static POOL_0_1: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            CURRENCY0.clone(),
            CURRENCY1.clone(),
            FEE.into(),
            TICK_SPACING,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });

    static POOL_1_ETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            ETHER.clone().into(),
            CURRENCY1.clone(),
            FEE.into(),
            TICK_SPACING,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });

    const TOKEN_ID: U256 = uint!(1_U256);
    static SLIPPAGE_TOLERANCE: Lazy<Percent> = Lazy::new(|| Percent::new(1, 100));
    const DEADLINE: U256 = uint!(123_U256);

    const MOCK_OWNER: Address = address!("000000000000000000000000000000000000000a");
    const MOCK_SPENDER: Address = address!("000000000000000000000000000000000000000b");
    const RECIPIENT: Address = address!("000000000000000000000000000000000000000c");

    fn common_options() -> CommonOptions {
        CommonOptions {
            slippage_tolerance: SLIPPAGE_TOLERANCE.clone(),
            deadline: DEADLINE,
            hook_data: Bytes::default(),
        }
    }

    fn mint_specific_options() -> AddLiquiditySpecificOptions {
        MintSpecificOptions {
            recipient: RECIPIENT,
            ..Default::default()
        }
        .into()
    }

    mod create_call_parameters {
        use super::*;

        #[test]
        fn succeeds() {
            let pool_key = Pool::get_pool_key(
                &CURRENCY0.clone(),
                &CURRENCY1.clone(),
                FEE.into(),
                TICK_SPACING,
                Address::ZERO,
            )
            .unwrap();

            let MethodParameters { calldata, value } =
                create_call_parameters(pool_key, *SQRT_PRICE_1_1);

            assert_eq!(calldata.to_vec(), hex!("0xf7020405000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000"));
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_with_nonzero_hook() {
            let hook = address!("1100000000000000000000000000000000002401");
            let pool_key = Pool::get_pool_key(
                &CURRENCY0.clone(),
                &CURRENCY1.clone(),
                FEE.into(),
                TICK_SPACING,
                hook,
            )
            .unwrap();

            let MethodParameters { calldata, value } =
                create_call_parameters(pool_key, *SQRT_PRICE_1_1);

            assert_eq!(calldata.to_vec(), hex!("0xf7020405000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c00000000000000000000000011000000000000000000000000000000000024010000000000000000000000000000000000000001000000000000000000000000"));
            assert_eq!(value, U256::ZERO);
        }
    }

    mod add_call_parameters {
        use super::*;
        use alloy_primitives::b256;

        #[test]
        #[should_panic(expected = "ZERO_LIQUIDITY")]
        fn throws_if_liquidity_is_0() {
            let mut position = Position::new(POOL_0_1.clone(), 0, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: mint_specific_options(),
                ..Default::default()
            };

            add_call_parameters(&mut position, options).unwrap();
        }

        #[test]
        #[should_panic(expected = "NATIVE_NOT_SET")]
        fn throws_if_pool_does_not_involve_ether_and_use_native_is_set() {
            let mut position =
                Position::new(POOL_0_1.clone(), 8888888, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                use_native: Some(ETHER.clone()),
                batch_permit: None,
                specific_opts: mint_specific_options(),
            };

            add_call_parameters(&mut position, options).unwrap();
        }

        #[test]
        #[should_panic(expected = "NATIVE_NOT_SET")]
        fn throws_if_pool_involves_ether_and_use_native_is_not_set() {
            let mut position =
                Position::new(POOL_1_ETH.clone(), 8888888, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: mint_specific_options(),
                ..Default::default()
            };

            add_call_parameters(&mut position, options).unwrap();
        }

        #[test]
        #[should_panic(expected = "NO_SQRT_PRICE")]
        fn throws_if_create_pool_is_true_but_there_is_no_sqrt_price_defined() {
            let mut position = Position::new(POOL_0_1.clone(), 1, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: MintSpecificOptions {
                    recipient: RECIPIENT,
                    create_pool: true,
                    ..Default::default()
                }
                .into(),
                ..Default::default()
            };

            add_call_parameters(&mut position, options).unwrap();
        }

        #[test]
        fn succeeds_for_mint() {
            let mut position =
                Position::new(POOL_0_1.clone(), 5000000, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: mint_specific_options(),
                ..Default::default()
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002020d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c00000000000000000000000000000000000000000000000000000000004c4b40000000000000000000000000000000000000000000000000000000000000752f000000000000000000000000000000000000000000000000000000000000752f000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000001800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002"));

            // Rebuild the calldata with the planner for the expected mint.
            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            planner.add_mint(
                &POOL_0_1,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(5000000_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );
            // Expect there to be a settle pair call afterwards
            planner.add_settle_pair(&POOL_0_1.currency0, &POOL_0_1.currency1);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_for_increase() {
            let mut position = Position::new(POOL_0_1.clone(), 666, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: ModifyPositionSpecificOptions { token_id: TOKEN_ID }.into(),
                ..Default::default()
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000220000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002000d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000029a0000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002"));

            // Rebuild the calldata with the planner for increase
            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            planner.add_increase(
                TOKEN_ID,
                uint!(666_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                Bytes::default(),
            );
            // Expect there to be a settle pair call afterwards
            planner.add_settle_pair(&POOL_0_1.currency0, &POOL_0_1.currency1);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_when_create_pool_is_true() {
            let mut position = Position::new(
                POOL_0_1.clone(),
                90000000000000_u128,
                -TICK_SPACING,
                TICK_SPACING,
            );

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: MintSpecificOptions {
                    recipient: RECIPIENT,
                    create_pool: true,
                    sqrt_price_x96: Some(*SQRT_PRICE_1_1),
                    migrate: false,
                }
                .into(),
                ..Default::default()
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xac9650d8000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000000c4f7020405000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000364dd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002020d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000051dac207a0000000000000000000000000000000000000000000000000000000007db8f27ddf0000000000000000000000000000000000000000000000000000007db8f27ddf000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000180000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000"));

            // The resulting calldata should be multicall with two calls: initializePool and
            // modifyLiquidities
            let calldata_arr: Vec<Bytes> = decode_multicall(&calldata).unwrap();
            // Expect initializePool to be called correctly
            assert_eq!(
                calldata_arr[0],
                encode_initialize_pool(POOL_0_1.pool_key.clone(), *SQRT_PRICE_1_1)
            );

            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            // Expect position to be minted correctly
            planner.add_mint(
                &POOL_0_1,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(90000000000000_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );
            planner.add_settle_pair(&POOL_0_1.currency0, &POOL_0_1.currency1);
            assert_eq!(
                calldata_arr[1],
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_when_use_native_is_set() {
            let mut position = Position::new(POOL_1_ETH.clone(), 1, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                use_native: Some(ETHER.clone()),
                batch_permit: None,
                specific_opts: mint_specific_options(),
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000380000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000003020d140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000220000000000000000000000000000000000000000000000000000000000000028000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000001800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001"));

            // Rebuild the data with the planner for the expected mint. MUST sweep since we are
            // using the native currency.
            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            // Expect position to be minted correctly
            planner.add_mint(
                &POOL_1_ETH,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(1_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );

            planner.add_settle_pair(&POOL_1_ETH.currency0, &POOL_1_ETH.currency1);
            planner.add_sweep(&POOL_1_ETH.currency0, MSG_SENDER);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, amount0_max);
        }

        #[test]
        fn succeeds_when_migrate_is_true() {
            let mut position = Position::new(POOL_0_1.clone(), 1, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                specific_opts: MintSpecificOptions {
                    recipient: RECIPIENT,
                    migrate: true,
                    ..Default::default()
                }
                .into(),
                ..Default::default()
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b00000000000000000000000000000000000000000000000000000000000004c0000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000005020b0b1414000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000026000000000000000000000000000000000000000000000000000000000000002e0000000000000000000000000000000000000000000000000000000000000036000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000018000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000c"));

            // Rebuild the data with the planner for the expected mint. MUST sweep since we are
            // using the native currency.
            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            // Expect position to be minted correctly
            planner.add_mint(
                &POOL_0_1,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(1_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );

            planner.add_settle(&POOL_0_1.currency0, false, None);
            planner.add_settle(&POOL_0_1.currency1, false, None);
            planner.add_sweep(&POOL_0_1.currency0, RECIPIENT);
            planner.add_sweep(&POOL_0_1.currency1, RECIPIENT);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_when_migrating_to_an_eth_position() {
            let mut position = Position::new(POOL_1_ETH.clone(), 1, -TICK_SPACING, TICK_SPACING);

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                use_native: Some(ETHER.clone()),
                batch_permit: None,
                specific_opts: MintSpecificOptions {
                    recipient: RECIPIENT,
                    migrate: true,
                    ..Default::default()
                }
                .into(),
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000052000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000602160b0b14140000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000028000000000000000000000000000000000000000000000000000000000000002c0000000000000000000000000000000000000000000000000000000000000034000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000042000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000180000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000c"));

            // Rebuild the data with the planner for the expected mint. MUST sweep since we are
            // using the native currency.
            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            // Expect position to be minted correctly
            planner.add_mint(
                &POOL_1_ETH,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(1_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );

            planner.add_unwrap(OPEN_DELTA);
            planner.add_settle(&POOL_1_ETH.currency0, false, None);
            planner.add_settle(&POOL_1_ETH.currency1, false, None);
            planner.add_sweep(POOL_1_ETH.currency0.wrapped(), RECIPIENT);
            planner.add_sweep(&POOL_1_ETH.currency1, RECIPIENT);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_for_batch_permit() {
            let mut position = Position::new(POOL_0_1.clone(), 1, -TICK_SPACING, TICK_SPACING);

            let batch_permit = BatchPermitOptions {
                owner: MOCK_OWNER,
                permit_batch: AllowanceTransferPermitBatch {
                    details: vec![],
                    spender: MOCK_SPENDER,
                    sigDeadline: DEADLINE,
                },
                signature: Bytes::from(b256!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                )),
            };

            let options = AddLiquidityOptions {
                common_opts: common_options(),
                use_native: None,
                batch_permit: Some(batch_permit.clone()),
                specific_opts: mint_specific_options(),
            };

            let MethodParameters { calldata, value } =
                add_call_parameters(&mut position, options).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xac9650d800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000124002a3e3a000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e00000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000b000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000364dd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002020d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000003c0000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc4000000000000000000000000000000000000000000000000000000000000003c000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000180000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000"));

            let calldata_arr: Vec<Bytes> = decode_multicall(&calldata).unwrap();
            // Expect permitBatch to be called correctly
            assert_eq!(
                calldata_arr[0],
                encode_permit_batch(
                    batch_permit.owner,
                    batch_permit.permit_batch,
                    batch_permit.signature,
                )
            );

            let MintAmounts {
                amount0: amount0_max,
                amount1: amount1_max,
            } = position
                .mint_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();
            planner.add_mint(
                &POOL_0_1,
                -TICK_SPACING,
                TICK_SPACING,
                uint!(1_U256),
                u128::try_from(amount0_max).unwrap(),
                u128::try_from(amount1_max).unwrap(),
                RECIPIENT,
                Bytes::default(),
            );
            planner.add_settle_pair(&POOL_0_1.currency0, &POOL_0_1.currency1);
            assert_eq!(
                calldata_arr[1],
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }
    }

    mod remove_call_parameters {
        use super::*;

        static POSITION: Lazy<Position> =
            Lazy::new(|| Position::new(POOL_0_1.clone(), 100, -TICK_SPACING, TICK_SPACING));

        fn remove_liq_options() -> RemoveLiquidityOptions {
            RemoveLiquidityOptions {
                common_opts: common_options(),
                token_id: TOKEN_ID,
                liquidity_percentage: Percent::new(1, 1),
                ..Default::default()
            }
        }

        fn partial_remove_options() -> RemoveLiquidityOptions {
            RemoveLiquidityOptions {
                common_opts: common_options(),
                token_id: TOKEN_ID,
                liquidity_percentage: SLIPPAGE_TOLERANCE.clone(),
                ..Default::default()
            }
        }

        fn burn_liq_options() -> RemoveLiquidityOptions {
            RemoveLiquidityOptions {
                burn_token: true,
                ..remove_liq_options()
            }
        }

        fn burn_liq_with_permit_options() -> RemoveLiquidityOptions {
            RemoveLiquidityOptions {
                permit: Some(NFTPermitOptions {
                    values: NFTPermitValues {
                        spender: MOCK_SPENDER,
                        tokenId: TOKEN_ID,
                        deadline: DEADLINE,
                        nonce: uint!(1_U256),
                    },
                    signature: Signature::from_raw_array(&[0_u8; 65]).unwrap(),
                }),
                ..burn_liq_options()
            }
        }

        #[test]
        #[should_panic(expected = "ZERO_LIQUIDITY")]
        fn throws_for_0_liquidity() {
            let zero_liquidity_position =
                Position::new(POOL_0_1.clone(), 0, -TICK_SPACING, TICK_SPACING);

            remove_call_parameters(&zero_liquidity_position, remove_liq_options()).unwrap();
        }

        #[test]
        #[should_panic(expected = "CANNOT_BURN")]
        fn throws_when_burn_is_true_but_liquidity_percentage_is_not_100_percent() {
            let full_liquidity_position =
                Position::new(POOL_0_1.clone(), 999, -TICK_SPACING, TICK_SPACING);

            let invalid_burn_options = RemoveLiquidityOptions {
                burn_token: true,
                liquidity_percentage: SLIPPAGE_TOLERANCE.clone(),
                token_id: TOKEN_ID,
                common_opts: common_options(),
                permit: None,
            };

            remove_call_parameters(&full_liquidity_position, invalid_burn_options).unwrap();
        }

        #[test]
        fn succeeds_for_burn() {
            let position = POSITION.clone();
            let MethodParameters { calldata, value } =
                remove_call_parameters(&position, burn_liq_options()).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000220000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002031100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001"));

            let (amount0_min, amount1_min) = position
                .burn_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();

            planner.add_burn(
                TOKEN_ID,
                u128::try_from(amount0_min).unwrap(),
                u128::try_from(amount1_min).unwrap(),
                Bytes::default(),
            );
            planner.add_take_pair(&*CURRENCY0, &*CURRENCY1, MSG_SENDER);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), burn_liq_options().deadline)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_for_remove_partial_liquidity() {
            let position = POSITION.clone();
            let MethodParameters { calldata, value } =
                remove_call_parameters(&position, partial_remove_options()).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000240000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002011100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001"));

            let (amount0_min, amount1_min) = position
                .burn_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();

            // remove 1% of 100, 1
            planner.add_decrease(
                TOKEN_ID,
                uint!(1_U256), // 1% of 100 liquidity
                u128::try_from(amount0_min).unwrap(),
                u128::try_from(amount1_min).unwrap(),
                Bytes::default(),
            );
            planner.add_take_pair(&*CURRENCY0, &*CURRENCY1, MSG_SENDER);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), partial_remove_options().deadline)
            );
            assert_eq!(value, U256::ZERO);
        }

        #[test]
        fn succeeds_for_burn_with_permit() {
            let position = POSITION.clone();
            let MethodParameters { calldata, value } =
                remove_call_parameters(&position, burn_liq_with_permit_options()).unwrap();

            assert_eq!(calldata.to_vec(), hex!("0xac9650d800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000001240f5730f1000000000000000000000000000000000000000000000000000000000000000b0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000041000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000284dd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000220000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002031100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000"));

            let (amount0_min, amount1_min) = position
                .burn_amounts_with_slippage(&SLIPPAGE_TOLERANCE.clone())
                .unwrap();

            let mut planner = V4PositionPlanner::default();

            planner.add_burn(
                TOKEN_ID,
                u128::try_from(amount0_min).unwrap(),
                u128::try_from(amount1_min).unwrap(),
                Bytes::default(),
            );
            planner.add_take_pair(&*CURRENCY0, &*CURRENCY1, MSG_SENDER);

            // The resulting calldata should be multicall with two calls:
            // ERC721Permit.permit and modifyLiquidities
            let calldata_arr: Vec<Bytes> = decode_multicall(&calldata).unwrap();
            // Expect ERC721Permit.permit to be called correctly
            let permit = burn_liq_with_permit_options().permit.unwrap();
            assert_eq!(
                calldata_arr[0],
                encode_erc721_permit(
                    permit.spender,
                    TOKEN_ID,
                    permit.deadline,
                    permit.nonce,
                    permit.signature.as_bytes().into(),
                )
            );
            // Expect modifyLiquidities to be called correctly
            assert_eq!(
                calldata_arr[1],
                encode_modify_liquidities(planner.0.finalize(), burn_liq_options().deadline)
            );
            assert_eq!(value, U256::ZERO);
        }
    }

    mod collect_call_parameters {
        use super::*;

        #[test]
        fn succeeds() {
            let position = Position::new(POOL_0_1.clone(), 100, -TICK_SPACING, TICK_SPACING);
            let MethodParameters { calldata, value } = collect_call_parameters(
                &position,
                CollectOptions {
                    common_opts: common_options(),
                    token_id: TOKEN_ID,
                    recipient: RECIPIENT,
                },
            );

            assert_eq!(calldata.to_vec(), hex!("0xdd46508f0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000007b0000000000000000000000000000000000000000000000000000000000000240000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000002011100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000c"));

            let mut planner = V4PositionPlanner::default();

            planner.add_decrease(TOKEN_ID, U256::ZERO, 0, 0, Bytes::default());
            planner.add_take_pair(&*CURRENCY0, &*CURRENCY1, RECIPIENT);

            assert_eq!(
                calldata,
                encode_modify_liquidities(planner.0.finalize(), DEADLINE)
            );
            assert_eq!(value, U256::ZERO);
        }
    }

    mod get_permit_data {
        use super::*;
        use alloy_primitives::b256;
        use alloy_sol_types::SolStruct;

        #[test]
        fn succeeds() {
            const PERMIT: NFTPermitValues = NFTPermitValues {
                spender: MOCK_SPENDER,
                tokenId: uint!(1_U256),
                deadline: uint!(123_U256),
                nonce: uint!(1_U256),
            };

            const PERMIT_DATA: NFTPermitData = get_permit_data(PERMIT, MOCK_OWNER, 1);

            assert_eq!(
                PERMIT_DATA.domain.name,
                Some("Uniswap V4 Positions NFT".into())
            );
            assert_eq!(PERMIT_DATA.domain.chain_id, Some(uint!(1_U256)));
            assert_eq!(PERMIT_DATA.domain.verifying_contract, Some(MOCK_OWNER));
            assert_eq!(PERMIT_DATA.values, PERMIT);

            // Compute the type hash by hashing the encoded type
            // ref https://github.com/Uniswap/v3-periphery/blob/main/contracts/base/ERC721Permit.sol
            assert_eq!(
                PERMIT.eip712_type_hash(),
                b256!("49ecf333e5b8c95c40fdafc95c1ad136e8914a8fb55e9dc8bb01eaa83a2df9ad")
            );
        }
    }
}
