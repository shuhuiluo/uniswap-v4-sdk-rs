use crate::prelude::{encode_route_to_path, Error, PathKey, Trade};
use alloy_primitives::{Bytes, U256};
use alloy_sol_types::{sol, SolValue};
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
pub enum Actions {
    // Pool actions
    // Liquidity actions
    INCREASE_LIQUIDITY(IncreaseLiquidityParams),
    DECREASE_LIQUIDITY(DecreaseLiquidityParams),
    MINT_POSITION(MintPositionParams),
    BURN_POSITION(BurnPositionParams),
    // Swapping
    SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams),
    SWAP_EXACT_IN(SwapExactInParams),
    SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams),
    SWAP_EXACT_OUT(SwapExactOutParams),

    // Closing deltas on the pool manager
    // Settling
    SETTLE(SettleParams),
    SETTLE_ALL(SettleAllParams),
    SETTLE_PAIR(SettlePairParams),
    // Taking
    TAKE(TakeParams),
    TAKE_ALL(TakeAllParams),
    TAKE_PORTION(TakePortionParams),
    TAKE_PAIR(TakePairParams),

    SETTLE_TAKE_PAIR(SettleTakePairParams),

    CLOSE_CURRENCY(CloseCurrencyParams),
    SWEEP(SweepParams),
}

sol! {
    #[derive(Debug, PartialEq)]
    struct PoolKeyStruct {
        address currency0;
        address currency1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    #[derive(Debug, PartialEq)]
    struct IncreaseLiquidityParams {
        uint256 tokenId;
        uint256 liquidity;
        uint128 amount0Max;
        uint128 amount1Max;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct DecreaseLiquidityParams {
        uint256 tokenId;
        uint256 liquidity;
        uint128 amount0Min;
        uint128 amount1Min;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct MintPositionParams {
        PoolKeyStruct poolKey;
        int24 tickLower;
        int24 tickUpper;
        uint256 liquidity;
        uint128 amount0Max;
        uint128 amount1Max;
        address owner;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct BurnPositionParams {
        uint256 tokenId;
        uint128 amount0Min;
        uint128 amount1Min;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct SwapExactInSingleParams {
        PoolKeyStruct poolKey;
        bool zeroForOne;
        uint128 amountIn;
        uint128 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct SwapExactInParams {
        address currencyIn;
        PathKey[] path;
        uint128 amountIn;
        uint128 amountOutMinimum;
    }

    #[derive(Debug, PartialEq)]
    struct SwapExactOutSingleParams {
        PoolKeyStruct poolKey;
        bool zeroForOne;
        uint128 amountOut;
        uint128 amountInMaximum;
        uint160 sqrtPriceLimitX96;
        bytes hookData;
    }

    #[derive(Debug, PartialEq)]
    struct SwapExactOutParams {
        address currencyOut;
        PathKey[] path;
        uint128 amountOut;
        uint128 amountInMaximum;
    }

    #[derive(Debug, PartialEq)]
    struct SettleParams {
        address currency;
        uint256 amount;
        bool payerIsUser;
    }

    #[derive(Debug, PartialEq)]
    struct SettleAllParams {
        address currency;
        uint256 maxAmount;
    }

    #[derive(Debug, PartialEq)]
    struct SettlePairParams {
        address currency0;
        address currency1;
    }

    #[derive(Debug, PartialEq)]
    struct TakeParams {
        address currency;
        address recipient;
        uint256 amount;
    }

    #[derive(Debug, PartialEq)]
    struct TakeAllParams {
        address currency;
        uint256 minAmount;
    }

    #[derive(Debug, PartialEq)]
    struct TakePortionParams {
        address currency;
        address recipient;
        uint256 bips;
    }

    #[derive(Debug, PartialEq)]
    struct TakePairParams {
        address currency0;
        address currency1;
        address recipient;
    }

    #[derive(Debug, PartialEq)]
    struct SettleTakePairParams {
        address settleCurrency;
        address takeCurrency;
    }

    #[derive(Debug, PartialEq)]
    struct CloseCurrencyParams {
        address currency;
    }

    #[derive(Debug, PartialEq)]
    struct SweepParams {
        address currency;
        address recipient;
    }

    #[derive(Debug, PartialEq)]
    struct FinalizeParams {
        bytes actions;
        bytes[] params;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct V4Planner {
    pub actions: Vec<u8>,
    pub params: Vec<Bytes>,
}

impl V4Planner {
    #[inline]
    pub fn add_action(&mut self, action: Actions) {
        let action = create_action(action);
        self.actions.push(action.action);
        self.params.push(action.encoded_input);
    }

    #[inline]
    pub fn add_trade<TInput, TOutput, TP>(
        &mut self,
        trade: &Trade<TInput, TOutput, TP>,
        slippage_tolerance: Option<Percent>,
    ) -> Result<(), Error>
    where
        TInput: BaseCurrency,
        TOutput: BaseCurrency,
        TP: TickDataProvider,
    {
        let exact_output = trade.trade_type == TradeType::ExactOutput;

        // exactInput we sometimes perform aggregated slippage checks, but not with exactOutput
        if exact_output {
            assert!(
                slippage_tolerance.is_some(),
                "ExactOut requires slippageTolerance"
            );
        }
        assert_eq!(
            trade.swaps.len(),
            1,
            "Only accepts Trades with 1 swap (must break swaps into individual trades)"
        );

        let currency_in = currency_address(trade.input_currency());
        let currency_out = currency_address(trade.output_currency());
        let path = encode_route_to_path(trade.route(), exact_output);

        self.add_action(if exact_output {
            Actions::SWAP_EXACT_OUT(SwapExactOutParams {
                currencyOut: currency_out,
                path,
                amountOut: trade.output_amount()?.quotient().to_u128().unwrap(),
                amountInMaximum: trade
                    .maximum_amount_in(slippage_tolerance.unwrap_or_default(), None)?
                    .quotient()
                    .to_u128()
                    .unwrap(),
            })
        } else {
            Actions::SWAP_EXACT_IN(SwapExactInParams {
                currencyIn: currency_in,
                path,
                amountIn: trade.input_amount()?.quotient().to_u128().unwrap(),
                amountOutMinimum: if let Some(slippage_tolerance) = slippage_tolerance {
                    trade
                        .minimum_amount_out(slippage_tolerance, None)?
                        .quotient()
                        .to_u128()
                        .unwrap()
                } else {
                    0
                },
            })
        });
        Ok(())
    }

    #[inline]
    pub fn add_settle(
        &mut self,
        currency: &impl BaseCurrency,
        payer_is_user: bool,
        amount: Option<U256>,
    ) {
        self.add_action(Actions::SETTLE(SettleParams {
            currency: currency_address(currency),
            amount: amount.unwrap_or_default(),
            payerIsUser: payer_is_user,
        }));
    }

    #[inline]
    pub fn add_take(
        &mut self,
        currency: &impl BaseCurrency,
        recipient: Address,
        amount: Option<U256>,
    ) {
        self.add_action(Actions::TAKE(TakeParams {
            currency: currency_address(currency),
            recipient,
            amount: amount.unwrap_or_default(),
        }));
    }

    #[inline]
    #[must_use]
    pub fn finalize(self) -> Bytes {
        FinalizeParams {
            actions: self.actions.into(),
            params: self.params,
        }
        .abi_encode()
        .into()
    }
}

fn currency_address(currency: &impl BaseCurrency) -> Address {
    if currency.is_native() {
        Address::ZERO
    } else {
        currency.wrapped().address()
    }
}

struct RouterAction {
    action: u8,
    encoded_input: Bytes,
}

macro_rules! router_action {
    ($action:expr, $params:expr) => {
        RouterAction {
            action: $action,
            encoded_input: $params.abi_encode().into(),
        }
    };
}

fn create_action(action: Actions) -> RouterAction {
    match action {
        Actions::INCREASE_LIQUIDITY(params) => router_action!(0x00, params),
        Actions::DECREASE_LIQUIDITY(params) => router_action!(0x01, params),
        Actions::MINT_POSITION(params) => router_action!(0x02, params),
        Actions::BURN_POSITION(params) => router_action!(0x03, params),
        Actions::SWAP_EXACT_IN_SINGLE(params) => router_action!(0x04, params),
        Actions::SWAP_EXACT_IN(params) => router_action!(0x05, params),
        Actions::SWAP_EXACT_OUT_SINGLE(params) => router_action!(0x06, params),
        Actions::SWAP_EXACT_OUT(params) => router_action!(0x07, params),
        Actions::SETTLE(params) => router_action!(0x09, params),
        Actions::SETTLE_ALL(params) => router_action!(0x10, params),
        Actions::SETTLE_PAIR(params) => router_action!(0x11, params),
        Actions::TAKE(params) => router_action!(0x12, params),
        Actions::TAKE_ALL(params) => router_action!(0x13, params),
        Actions::TAKE_PORTION(params) => router_action!(0x14, params),
        Actions::TAKE_PAIR(params) => router_action!(0x15, params),
        Actions::SETTLE_TAKE_PAIR(params) => router_action!(0x16, params),
        Actions::CLOSE_CURRENCY(params) => router_action!(0x17, params),
        Actions::SWEEP(params) => router_action!(0x19, params),
    }
}
