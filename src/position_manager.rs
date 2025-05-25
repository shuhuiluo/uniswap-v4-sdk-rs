use crate::prelude::{Error, *};
use alloc::vec::Vec;
use alloy_primitives::{address, Address, Bytes, Signature, U160, U256};
use alloy_sol_types::{eip712_domain, SolCall};
use derive_more::{Deref, DerefMut};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonOptions {
    /// How much the pool price is allowed to move from the specified action.
    pub slippage_tolerance: Percent,
    /// When the transaction expires, in epoch seconds.
    pub deadline: U256,
    /// Optional data to pass to hooks.
    pub hook_data: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifyPositionSpecificOptions {
    /// Indicates the ID of the position to increase liquidity for.
    pub token_id: U256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddLiquiditySpecificOptions {
    Mint(MintSpecificOptions),
    Increase(ModifyPositionSpecificOptions),
}

/// Options for producing the calldata to add liquidity.
#[derive(Debug, Clone, PartialEq, Deref, DerefMut)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deref, DerefMut)]
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
        "Native currency must match pool currency0 or not be used when currency0 is not native"
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
