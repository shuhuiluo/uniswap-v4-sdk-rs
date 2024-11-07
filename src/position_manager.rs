use core::fmt::Debug;

use crate::{
    abi::{self, PoolKey},
    prelude::Position,
    utils::{
        abi as Interface, v4_postition_planner::V4PositionPlanner,
        V4Planner,
    },
};

use super::entities::{position, Pool};
use alloc::{boxed::Box, string::String, vec::Vec};
use alloy_primitives::{aliases::U48, Address, Bytes, Signature, I256, U128, U160, U256};
use alloy_sol_types::{
    abi::{encode, encode_params, token::DynSeqToken},
    SolValue,
};
use uniswap_sdk_core::{
    entities::native_currency,
    prelude::{
        BaseCurrencyCore, BigDecimal, BigInt, Currency, FractionBase, NativeCurrency, Percent, Zero,
    },
};
use uniswap_v3_sdk::{multicall, prelude::Multicall, utils::MethodParameters};

pub trait CommonOptions {
    fn slippage_tolerance(&self) -> &Percent;
    fn hook_data(&self) -> Option<Bytes>;
    fn deadline(&self) -> U256;
}

pub trait ModifyPositionSpecificOptions {
    fn token_id(&self) -> U256;
}

pub trait MintSpecificOptions {
    fn recipient(&self) -> Address;
    fn create_pool(&self) -> Option<bool>;
    fn sqrt_price_x96(&self) -> Option<U256>;
    fn migrate(&self) -> Option<bool>;
}

pub trait CommonAddLiquidityOptions {
    fn use_native(&self) -> Option<&Currency>;
    fn batch_permit(&self) -> Option<&BatchPermitOptions>;
}

pub trait CollectSpecificOptions {
    fn token_id(&self) -> U256;
    fn recipient(&self) -> Address;
}

pub trait TransferOption {
    fn sender(&self) -> Address;
    fn recipient(&self) -> Address;
    fn token_id(&self) -> U256;
}

pub trait RemoveLiquiditySpecificOptions {
    fn liquidity_percentage(&self) -> &Percent;
    fn burn_token(&self) -> Option<bool>;
    fn permit(&self) -> Option<&NFTPermitOptions>;
}

#[derive(Debug, Clone, Copy)]
pub struct PermitDetails {
    pub token: Address,
    pub amount: U160,
    pub expiration: U48,
    pub nonce: U48,
}

#[derive(Debug, Copy, Clone)]
pub struct AllowanceTransferPermitSingle {
    pub details: PermitDetails,
    pub spender: Address,
    pub sig_deadline: U256,
}

#[derive(Debug, Clone)]
pub struct AllowanceTransferPermitBatch {
    pub details: Vec<PermitDetails>,
    pub spender: Address,
    pub sig_deadline: U256,
}

#[derive(Debug)]
pub struct BatchPermitOptions {
    pub owner: Address,
    pub permit_batch: AllowanceTransferPermitBatch,
    pub signature: U256,
}

#[derive(Debug, Clone, Copy)]
pub struct NFTPermitValues {
    spender: Address,
    token_id: U256,
    deadline: U256,
    nonce: U256,
}

#[derive(Debug, Clone, Copy)]
pub struct NFTPermitOptions {
    values: NFTPermitValues,
    signature: U256,
}

#[derive(Debug, Clone, Copy)]
pub struct V4PositionManager;

#[derive(Debug)]
pub struct MintOptionsImpl {
    slippage_tolerance: Percent,
    hook_data: Option<Bytes>,
    deadline: U256,
    use_native: Option<Currency>,
    batch_permit: Option<BatchPermitOptions>,
    recipient: Address,
    create_pool: Option<bool>,
    sqrt_price_x96: Option<U256>,
    migrate: Option<bool>,
}

impl CommonOptions for MintOptionsImpl {
    fn slippage_tolerance(&self) -> &Percent {
        &self.slippage_tolerance
    }

    fn hook_data(&self) -> Option<Bytes> {
        self.hook_data.clone()
    }

    fn deadline(&self) -> U256 {
        self.deadline
    }
}

impl CommonAddLiquidityOptions for MintOptionsImpl {
    fn use_native(&self) -> Option<&Currency> {
        self.use_native.as_ref()
    }

    fn batch_permit(&self) -> Option<&BatchPermitOptions> {
        self.batch_permit.as_ref()
    }
}

impl MintSpecificOptions for MintOptionsImpl {
    fn recipient(&self) -> Address {
        self.recipient
    }

    fn create_pool(&self) -> Option<bool> {
        self.create_pool
    }

    fn sqrt_price_x96(&self) -> Option<U256> {
        self.sqrt_price_x96
    }

    fn migrate(&self) -> Option<bool> {
        self.migrate
    }
}

#[derive(Debug)]
pub struct IncreaseLiquidityOptionsImpl {
    slippage_tolerance: Percent,
    hook_data: Option<Bytes>,
    deadline: U256,
    use_native: Option<Currency>,
    batch_permit: Option<BatchPermitOptions>,
    token_id: U256,
}

impl CommonOptions for IncreaseLiquidityOptionsImpl {
    fn slippage_tolerance(&self) -> &Percent {
        &self.slippage_tolerance
    }

    fn hook_data(&self) -> Option<Bytes> {
        self.hook_data.clone()
    }

    fn deadline(&self) -> U256 {
        self.deadline
    }
}

impl CommonAddLiquidityOptions for IncreaseLiquidityOptionsImpl {
    fn use_native(&self) -> Option<&Currency> {
        self.use_native.as_ref()
    }

    fn batch_permit(&self) -> Option<&BatchPermitOptions> {
        self.batch_permit.as_ref()
    }
}

impl ModifyPositionSpecificOptions for IncreaseLiquidityOptionsImpl {
    fn token_id(&self) -> U256 {
        self.token_id
    }
}

///COMBINED TRAITS///

pub trait CombinedTrait:
    MintSpecificOptions
    + CommonOptions
    + CommonAddLiquidityOptions
    + ModifyPositionSpecificOptions
    + Debug
{
}
pub trait MintOptions: CommonOptions + CommonAddLiquidityOptions + MintSpecificOptions {}

pub trait IncreaseLiquidityOptions:
    CommonOptions + CommonAddLiquidityOptions + ModifyPositionSpecificOptions
{
}

#[derive(Debug)]
pub enum AddLiquidityOptions {
    MintOptions(Box<dyn CombinedTrait>),
    IncreaseLiquidityOptions(Box<dyn CombinedTrait>),
}

pub trait RemoveLiquidityOptions:
    CommonOptions + CommonAddLiquidityOptions + ModifyPositionSpecificOptions
{
}

pub trait CollectOptions: CommonOptions + CollectSpecificOptions {}

impl V4PositionManager {
    pub fn create_call_parameters(pool_key: PoolKey, sqrt_price_x96: U256) -> MethodParameters {
        MethodParameters {
            calldata: Self::encode_initialize_pool(pool_key, sqrt_price_x96),
            value: U256::ZERO,
        }
    }

    pub fn add_call_parameters(
        mut position: position::Position,
        options: AddLiquidityOptions,
    ) -> MethodParameters {
        if position.liquidity.is_zero() {
            panic!("ZERO_LIQUIDITY");
        }
        let mut calldata_list: Vec<u8> = Vec::new();
        let mut planner = V4PositionPlanner::default();
        // Any sweeping must happen after the settling.
        let mut value: u128 = 0;

        match options {
            AddLiquidityOptions::MintOptions(mint_specific_options) => {
                if mint_specific_options.create_pool().unwrap_or(false) {
                    if let Some(sqrt_price) = mint_specific_options.sqrt_price_x96() {
                        calldata_list.extend(Self::encode_initialize_pool(
                            position.clone().pool.pool_key,
                            sqrt_price,
                        ));
                    }
                }

                // Calculate maximum amounts with slippage
                let maximum_amt = position
                    .mint_amounts_with_slippage(mint_specific_options.slippage_tolerance())
                    .expect("mint specific amount");

                if !mint_specific_options.batch_permit().is_none() {
                    //We can use unwrap since we already confirm it's not none
                    calldata_list.extend(
                        V4PositionManager::encode_permit_batch(
                            mint_specific_options.batch_permit().unwrap().owner,
                            mint_specific_options
                                .batch_permit()
                                .unwrap()
                                .permit_batch
                                .clone(),
                            mint_specific_options.batch_permit().unwrap().signature,
                        )
                        .abi_encode(),
                    );
                }

                let amt_1 = U128::from(maximum_amt.amount0).to();
                let amt_2 = U128::from(maximum_amt.amount1).to();

                //ISMINT LOGIC
                if mint_specific_options.recipient() != Address::ZERO {
                    planner.add_mint(
                        &position.pool,
                        position.tick_lower,
                        position.tick_upper,
                        U256::from(position.liquidity),
                        amt_1,
                        amt_2,
                        mint_specific_options.recipient(),
                        mint_specific_options
                            .hook_data()
                            .unwrap_or(Bytes::default()),
                    );
                }

                // Handle native currency if specified
                if !mint_specific_options.use_native().is_none() {
                    if position.pool.currency0.is_native() {
                        value = amt_1;
                        planner.add_sweep(&position.pool.currency0, Address::ZERO);
                    } else {
                        value = amt_2;
                        planner.add_sweep(&position.pool.currency1, Address::ZERO);
                    }
                }
            }
            AddLiquidityOptions::IncreaseLiquidityOptions(modify_specific_options) => {
                let maximum_amt = position
                    .mint_amounts_with_slippage(modify_specific_options.slippage_tolerance())
                    .expect("mint specific amount");
                let amt_1 = U128::from(maximum_amt.amount0).to();
                let amt_2 = U128::from(maximum_amt.amount1).to();

                planner.add_increase(
                    modify_specific_options.token_id(),
                    U256::from(position.liquidity),
                    amt_1,
                    amt_2,
                    modify_specific_options
                        .hook_data()
                        .unwrap_or(Bytes::default()),
                );

                planner.add_settle_pair(&position.pool.currency0, &position.pool.currency1);

                // Handle native currency if specified
                if !modify_specific_options.use_native().is_none() {
                    if position.pool.currency0.is_native() {
                        value = amt_1;
                        planner.add_sweep(&position.pool.currency0, Address::ZERO);
                    } else {
                        value = amt_2;
                        planner.add_sweep(&position.pool.currency1, Address::ZERO);
                    }
                }
            }
        }

        MethodParameters {
            calldata: todo!(),
            value: U256::from(value),
        }
    }

    pub fn remove_call_parameters(
        position: Position,
        options: impl RemoveLiquiditySpecificOptions + CombinedTrait,
    ) -> MethodParameters {
        let mut calldata_list: Vec<u8> = Vec::new();
        let mut planner = V4PositionPlanner::new();
        let token_id = options.token_id();

        if options.burn_token().unwrap_or(false) {
            assert!(
                options
                    .liquidity_percentage()
                    .to_decimal()
                    .eq(&BigDecimal::from(1)),
                "CANNOT BURN"
            );

            if !options.permit().is_none() {
                //since we've alreadt checked, we can use an unwrap
                calldata_list.extend(
                    V4PositionManager::encode_erc721_permit(
                        options.permit().unwrap().values.spender,
                        options.permit().unwrap().values.token_id,
                        options.permit().unwrap().values.deadline,
                        options.permit().unwrap().values.nonce,
                        options.permit().unwrap().signature,
                    )
                    .abi_encode(),
                );
            }

            // slippage-adjusted amounts derived from current position liquidity
            let amounts = position
                .burn_amounts_with_slippage(options.slippage_tolerance())
                .expect("AMOUNTS");

            let amount_0 = (U128::from(amounts.0)).to();

            let amount_1 = (U128::from(amounts.1)).to();

            //BYTES::NEW() is valid since it creates a new empty bytes
            planner.add_burn(
                token_id,
                amount_0,
                amount_1,
                options.hook_data().unwrap_or(Bytes::new()),
            );
        } else {
            let partial_position = Position::new(
                position.clone().pool,
                position.liquidity,
                position.tick_lower,
                position.tick_upper,
            );

            assert!(partial_position.liquidity > 0, "ZERO_LIQUIDITY");

            let amounts = partial_position
                .burn_amounts_with_slippage(options.slippage_tolerance())
                .expect("BURN AMOUNTS");

            let amt0 = U128::from(amounts.0).to();
            let amt1 = U128::from(amounts.1).to();

            planner.add_decrease(
                token_id,
                U256::from(partial_position.liquidity),
                amt0,
                amt1,
                options.hook_data().unwrap_or(Bytes::new()),
            );
        }

        planner.add_take_pair(
            &position.pool.clone().currency0,
            &position.pool.currency1,
            Address::ZERO,
        );
        calldata_list.extend(V4PositionManager::encode_modify_liquidities(planner.planner.finalize(), options.deadline()).abi_encode());

        MethodParameters {
            calldata: todo!(),
            value: U256::ZERO,
        }
    }

    pub fn collect_call_parameters(position: Position, options: impl CollectOptions) -> MethodParameters {
        let mut calldata_list: Vec<u8> = Vec::new();
        let mut planner = V4PositionPlanner::new();

        let token_id = options.token_id();
        let recipient = options.recipient();

        /*
         * To collect fees in V4, we need to:
         * - encode a decrease liquidity by 0
         * - and encode a TAKE_PAIR
         */
        planner.add_decrease(
            token_id,
            U256::ZERO,
            0,
            0,
            options.hook_data().unwrap_or(Bytes::new()),
        );

        planner.add_take_pair(
            &position.pool.currency0,
            &position.pool.currency1,
            recipient,
        );

        calldata_list.extend(V4PositionManager::encode_modify_liquidities(planner.planner.finalize(), options.deadline()).abi_encode());

        MethodParameters {
            calldata: todo!(),
            value: U256::ZERO,
        }
    }

    pub fn encode_initialize_pool(pool_key: PoolKey, sqrt_price_x96: U256) -> Bytes {
        todo!()
    }

    pub fn encode_modify_liquidities(unlockData: Bytes, deadline: U256) {
        todo!()
    }

    pub fn encode_permit_batch(
        owner: Address,
        permit_batch: AllowanceTransferPermitBatch,
        signature: U256,
    ) -> Vec<u8> {
        let mut arr = Vec::new();
        for i in permit_batch.details.iter() {
            arr.push(Interface::PermitDetails {
                token: i.token,
                amount: i.amount,
                expiration: i.expiration,
                nonce: i.nonce,
            });
        }
        let b = Interface::PermitBatch {
            spender: owner,
            details: arr,
            sigDeadline: signature,
        }
        .abi_encode();
        b
    }

    pub fn encode_erc721_permit(
        spender: Address,
        token_id: U256,
        deadline: U256,
        nonce: U256,
        signature: U256,
    ) {
        todo!()
    }
}
