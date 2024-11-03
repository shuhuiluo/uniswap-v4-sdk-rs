use crate::prelude::{encode_route_to_path, Error, Trade, *};
use alloy_primitives::{Bytes, U256};
use alloy_sol_types::SolValue;
use uniswap_sdk_core::prelude::*;
use uniswap_v3_sdk::prelude::*;

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Actions {
    // Pool actions
    // Liquidity actions
    INCREASE_LIQUIDITY(IncreaseLiquidityParams) = 0x00,
    DECREASE_LIQUIDITY(DecreaseLiquidityParams) = 0x01,
    MINT_POSITION(MintPositionParams) = 0x02,
    BURN_POSITION(BurnPositionParams) = 0x03,
    // Swapping
    SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams) = 0x04,
    SWAP_EXACT_IN(SwapExactInParams) = 0x05,
    SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams) = 0x06,
    SWAP_EXACT_OUT(SwapExactOutParams) = 0x07,

    // Closing deltas on the pool manager
    // Settling
    SETTLE(SettleParams) = 0x09,
    SETTLE_ALL(SettleAllParams) = 0x10,
    SETTLE_PAIR(SettlePairParams) = 0x11,
    // Taking
    TAKE(TakeParams) = 0x12,
    TAKE_ALL(TakeAllParams) = 0x13,
    TAKE_PORTION(TakePortionParams) = 0x14,
    TAKE_PAIR(TakePairParams) = 0x15,

    SETTLE_TAKE_PAIR(SettleTakePairParams) = 0x16,

    CLOSE_CURRENCY(CloseCurrencyParams) = 0x17,
    SWEEP(SweepParams) = 0x19,
}

/// https://doc.rust-lang.org/error_codes/E0732.html
#[inline]
const fn discriminant(v: &Actions) -> u8 {
    unsafe { *(v as *const Actions as *const u8) }
}

impl Actions {
    #[inline]
    pub const fn command(&self) -> u8 {
        discriminant(self)
    }

    #[inline]
    pub fn abi_encode(&self) -> Bytes {
        match self {
            Self::INCREASE_LIQUIDITY(params) => params.abi_encode(),
            Self::DECREASE_LIQUIDITY(params) => params.abi_encode(),
            Self::MINT_POSITION(params) => params.abi_encode(),
            Self::BURN_POSITION(params) => params.abi_encode(),
            Self::SWAP_EXACT_IN_SINGLE(params) => params.abi_encode(),
            Self::SWAP_EXACT_IN(params) => params.abi_encode(),
            Self::SWAP_EXACT_OUT_SINGLE(params) => params.abi_encode(),
            Self::SWAP_EXACT_OUT(params) => params.abi_encode(),
            Self::SETTLE(params) => params.abi_encode(),
            Self::SETTLE_ALL(params) => params.abi_encode(),
            Self::SETTLE_PAIR(params) => params.abi_encode(),
            Self::TAKE(params) => params.abi_encode(),
            Self::TAKE_ALL(params) => params.abi_encode(),
            Self::TAKE_PORTION(params) => params.abi_encode(),
            Self::TAKE_PAIR(params) => params.abi_encode(),
            Self::SETTLE_TAKE_PAIR(params) => params.abi_encode(),
            Self::CLOSE_CURRENCY(params) => params.abi_encode(),
            Self::SWEEP(params) => params.abi_encode(),
        }
        .into()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct V4Planner {
    pub actions: Vec<u8>,
    pub params: Vec<Bytes>,
}

impl V4Planner {
    #[inline]
    pub fn add_action(&mut self, action: &Actions) {
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

        self.add_action(
            &(if exact_output {
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
            }),
        );
        Ok(())
    }

    #[inline]
    pub fn add_settle(
        &mut self,
        currency: &impl BaseCurrency,
        payer_is_user: bool,
        amount: Option<U256>,
    ) {
        self.add_action(&Actions::SETTLE(SettleParams {
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
        self.add_action(&Actions::TAKE(TakeParams {
            currency: currency_address(currency),
            recipient,
            amount: amount.unwrap_or_default(),
        }));
    }

    #[inline]
    #[must_use]
    pub fn finalize(self) -> Bytes {
        ActionsParams {
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

fn create_action(action: &Actions) -> RouterAction {
    RouterAction {
        action: action.command(),
        encoded_input: action.abi_encode(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminant() {
        assert_eq!(
            discriminant(&Actions::INCREASE_LIQUIDITY(Default::default())),
            0x00
        );
        assert_eq!(
            discriminant(&Actions::DECREASE_LIQUIDITY(Default::default())),
            0x01
        );
        assert_eq!(
            discriminant(&Actions::MINT_POSITION(Default::default())),
            0x02
        );
        assert_eq!(
            discriminant(&Actions::BURN_POSITION(Default::default())),
            0x03
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_IN_SINGLE(Default::default())),
            0x04
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_IN(Default::default())),
            0x05
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_OUT_SINGLE(Default::default())),
            0x06
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_OUT(Default::default())),
            0x07
        );
        assert_eq!(discriminant(&Actions::SETTLE(Default::default())), 0x09);
        assert_eq!(discriminant(&Actions::SETTLE_ALL(Default::default())), 0x10);
        assert_eq!(
            discriminant(&Actions::SETTLE_PAIR(Default::default())),
            0x11
        );
        assert_eq!(discriminant(&Actions::TAKE(Default::default())), 0x12);
        assert_eq!(discriminant(&Actions::TAKE_ALL(Default::default())), 0x13);
        assert_eq!(
            discriminant(&Actions::TAKE_PORTION(Default::default())),
            0x14
        );
        assert_eq!(discriminant(&Actions::TAKE_PAIR(Default::default())), 0x15);
        assert_eq!(
            discriminant(&Actions::SETTLE_TAKE_PAIR(Default::default())),
            0x16
        );
        assert_eq!(
            discriminant(&Actions::CLOSE_CURRENCY(Default::default())),
            0x17
        );
        assert_eq!(discriminant(&Actions::SWEEP(Default::default())), 0x19);
    }
}
