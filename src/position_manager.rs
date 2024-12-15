use crate::prelude::*;
use alloy_primitives::{Address, Bytes, PrimitiveSignature, U160, U256};
use alloy_sol_types::{eip712_domain, SolCall};
use derive_more::{Deref, DerefMut};
use uniswap_sdk_core::prelude::{Ether, Percent};
use uniswap_v3_sdk::{
    entities::TickDataProvider,
    prelude::{IERC721Permit, MethodParameters},
};

pub use uniswap_v3_sdk::prelude::NFTPermitData;

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
    pub signature: PrimitiveSignature,
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

#[inline]
pub fn add_call_parameters<TP>(
    _position: Position<TP>,
    _options: AddLiquidityOptions,
) -> MethodParameters
where
    TP: TickDataProvider,
{
    unimplemented!("add_call_parameters")
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
/// use alloy_primitives::{address, b256, uint, PrimitiveSignature, B256};
/// use alloy_signer::SignerSync;
/// use alloy_signer_local::PrivateKeySigner;
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
/// let hash: B256 = data.values.eip712_signing_hash(&data.domain);
///
/// let signer = PrivateKeySigner::random();
/// let signature: PrimitiveSignature = signer.sign_hash_sync(&hash).unwrap();
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
