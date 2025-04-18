use crate::prelude::{encode_route_to_path, Error, Trade, *};
use alloy_primitives::{Bytes, U256};
use alloy_sol_types::SolValue;
use num_traits::ToPrimitive;
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
    SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams) = 0x06,
    SWAP_EXACT_IN(SwapExactInParams) = 0x07,
    SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams) = 0x08,
    SWAP_EXACT_OUT(SwapExactOutParams) = 0x09,

    // Closing deltas on the pool manager
    // Settling
    SETTLE(SettleParams) = 0x0b,
    SETTLE_ALL(SettleAllParams) = 0x0c,
    SETTLE_PAIR(SettlePairParams) = 0x0d,
    // Taking
    TAKE(TakeParams) = 0x0e,
    TAKE_ALL(TakeAllParams) = 0x0f,
    TAKE_PORTION(TakePortionParams) = 0x10,
    TAKE_PAIR(TakePairParams) = 0x11,

    CLOSE_CURRENCY(CloseCurrencyParams) = 0x12,
    SWEEP(SweepParams) = 0x14,
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
            Self::CLOSE_CURRENCY(params) => params.abi_encode(),
            Self::SWEEP(params) => params.abi_encode(),
        }
        .into()
    }

    #[inline]
    pub fn abi_decode(command: u8, data: &Bytes) -> Result<Self, Error> {
        let data = data.iter().as_slice();
        Ok(match command {
            0x00 => Self::INCREASE_LIQUIDITY(IncreaseLiquidityParams::abi_decode(data, true)?),
            0x01 => Self::DECREASE_LIQUIDITY(DecreaseLiquidityParams::abi_decode(data, true)?),
            0x02 => Self::MINT_POSITION(MintPositionParams::abi_decode(data, true)?),
            0x03 => Self::BURN_POSITION(BurnPositionParams::abi_decode(data, true)?),
            0x06 => Self::SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams::abi_decode(data, true)?),
            0x07 => Self::SWAP_EXACT_IN(SwapExactInParams::abi_decode(data, true)?),
            0x08 => Self::SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams::abi_decode(data, true)?),
            0x09 => Self::SWAP_EXACT_OUT(SwapExactOutParams::abi_decode(data, true)?),
            0x0b => Self::SETTLE(SettleParams::abi_decode(data, true)?),
            0x0c => Self::SETTLE_ALL(SettleAllParams::abi_decode(data, true)?),
            0x0d => Self::SETTLE_PAIR(SettlePairParams::abi_decode(data, true)?),
            0x0e => Self::TAKE(TakeParams::abi_decode(data, true)?),
            0x0f => Self::TAKE_ALL(TakeAllParams::abi_decode(data, true)?),
            0x10 => Self::TAKE_PORTION(TakePortionParams::abi_decode(data, true)?),
            0x11 => Self::TAKE_PAIR(TakePairParams::abi_decode(data, true)?),
            0x12 => Self::CLOSE_CURRENCY(CloseCurrencyParams::abi_decode(data, true)?),
            0x14 => Self::SWEEP(SweepParams::abi_decode(data, true)?),
            _ => return Err(Error::InvalidAction(command)),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct V4Planner {
    pub actions: Vec<u8>,
    pub params: Vec<Bytes>,
}

impl V4Planner {
    #[inline]
    pub fn add_action(&mut self, action: &Actions) -> &mut Self {
        self.actions.push(action.command());
        self.params.push(action.abi_encode());
        self
    }

    #[inline]
    pub fn add_trade<TInput, TOutput, TP>(
        &mut self,
        trade: &Trade<TInput, TOutput, TP>,
        slippage_tolerance: Option<Percent>,
    ) -> Result<&mut Self, Error>
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

        let route = trade.route();
        let currency_in = currency_address(&route.path_input);
        let currency_out = currency_address(&route.path_output);
        let path = encode_route_to_path(route, exact_output);

        Ok(self.add_action(
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
        ))
    }

    #[inline]
    pub fn add_settle(
        &mut self,
        currency: &impl BaseCurrency,
        payer_is_user: bool,
        amount: Option<U256>,
    ) -> &mut Self {
        self.add_action(&Actions::SETTLE(SettleParams {
            currency: currency_address(currency),
            amount: amount.unwrap_or_default(),
            payerIsUser: payer_is_user,
        }))
    }

    #[inline]
    pub fn add_take(
        &mut self,
        currency: &impl BaseCurrency,
        recipient: Address,
        amount: Option<U256>,
    ) -> &mut Self {
        self.add_action(&Actions::TAKE(TakeParams {
            currency: currency_address(currency),
            recipient,
            amount: amount.unwrap_or_default(),
        }))
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
        currency.address()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{currency_amount, prelude::Pool, tests::*};
    use alloy_primitives::hex;
    use once_cell::sync::Lazy;

    static USDC_WETH: Lazy<Pool<Vec<Tick>>> = Lazy::new(|| {
        Pool::new_with_tick_data_provider(
            USDC.clone().into(),
            WETH.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            1_000_000_000 * ONE_ETHER,
            TICK_LIST.clone(),
        )
        .unwrap()
    });
    static DAI_USDC: Lazy<Pool<Vec<Tick>>> = Lazy::new(|| {
        Pool::new_with_tick_data_provider(
            USDC.clone().into(),
            DAI.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            1_000_000_000 * ONE_ETHER,
            TICK_LIST.clone(),
        )
        .unwrap()
    });
    static DAI_WETH: Lazy<Pool<Vec<Tick>>> = Lazy::new(|| {
        Pool::new_with_tick_data_provider(
            WETH.clone().into(),
            DAI.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            encode_sqrt_ratio_x96(1, 1),
            ONE_ETHER,
            TICK_LIST.clone(),
        )
        .unwrap()
    });

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
            0x06
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_IN(Default::default())),
            0x07
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_OUT_SINGLE(Default::default())),
            0x08
        );
        assert_eq!(
            discriminant(&Actions::SWAP_EXACT_OUT(Default::default())),
            0x09
        );
        assert_eq!(discriminant(&Actions::SETTLE(Default::default())), 0x0b);
        assert_eq!(discriminant(&Actions::SETTLE_ALL(Default::default())), 0x0c);
        assert_eq!(
            discriminant(&Actions::SETTLE_PAIR(Default::default())),
            0x0d
        );
        assert_eq!(discriminant(&Actions::TAKE(Default::default())), 0x0e);
        assert_eq!(discriminant(&Actions::TAKE_ALL(Default::default())), 0x0f);
        assert_eq!(
            discriminant(&Actions::TAKE_PORTION(Default::default())),
            0x10
        );
        assert_eq!(discriminant(&Actions::TAKE_PAIR(Default::default())), 0x11);
        assert_eq!(
            discriminant(&Actions::CLOSE_CURRENCY(Default::default())),
            0x12
        );
        assert_eq!(discriminant(&Actions::SWEEP(Default::default())), 0x14);
    }

    #[test]
    fn test_add_action_encode_v4_exact_in_single_swap() {
        let mut planner = V4Planner::default();
        planner.add_action(&Actions::SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams {
            poolKey: USDC_WETH.pool_key.clone(),
            zeroForOne: true,
            amountIn: ONE_ETHER,
            amountOutMinimum: ONE_ETHER / 2,
            hookData: Bytes::default(),
        }));
        assert_eq!(planner.actions, vec![0x06]);
        assert_eq!(
            planner.params[0],
            hex!("0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc20000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000de0b6b3a764000000000000000000000000000000000000000000000000000006f05b59d3b2000000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000000").to_vec()
        );
    }

    mod add_settle {
        use super::*;
        use alloy_primitives::uint;

        #[test]
        fn completes_v4_settle_without_specified_amount() {
            let mut planner = V4Planner::default();
            planner.add_settle(&DAI.clone(), true, None);
            assert_eq!(planner.actions, vec![0x0b]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001").to_vec()
            );
        }

        #[test]
        fn completes_v4_settle_with_specified_amount() {
            let mut planner = V4Planner::default();
            planner.add_settle(&DAI.clone(), true, Some(uint!(8_U256)));
            assert_eq!(planner.actions, vec![0x0b]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f00000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000001").to_vec()
            );
        }

        #[test]
        fn completes_v4_settle_with_payer_is_user_as_false() {
            let mut planner = V4Planner::default();
            planner.add_settle(&DAI.clone(), false, Some(uint!(8_U256)));
            assert_eq!(planner.actions, vec![0x0b]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f00000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
        }
    }

    mod add_take {
        use super::*;
        use alloy_primitives::{address, uint};

        #[test]
        fn completes_v4_take_without_specified_amount() {
            let mut planner = V4Planner::default();
            planner.add_take(
                &DAI.clone(),
                address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                None,
            );
            assert_eq!(planner.actions, vec![0x0e]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
        }

        #[test]
        fn completes_v4_take_with_specified_amount() {
            let mut planner = V4Planner::default();
            planner.add_take(
                &DAI.clone(),
                address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                Some(uint!(8_U256)),
            );
            assert_eq!(planner.actions, vec![0x0e]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0000000000000000000000000000000000000000000000000000000000000008").to_vec()
            );
        }
    }

    mod add_trade {
        use super::*;
        use crate::{create_route, trade_from_route};

        #[tokio::test]
        async fn completes_v4_exact_in_2_hop_swap_same_results_as_add_action() {
            let route = create_route!(DAI_USDC, USDC_WETH; DAI, WETH);

            // encode with addAction function
            let mut planner = V4Planner::default();
            planner.add_action(&Actions::SWAP_EXACT_IN(SwapExactInParams {
                currencyIn: DAI.address,
                path: encode_route_to_path(&route, false),
                amountIn: ONE_ETHER,
                amountOutMinimum: 0,
            }));

            // encode with addTrade function
            let trade = trade_from_route!(
                route,
                currency_amount!(DAI, ONE_ETHER),
                TradeType::ExactInput
            );
            let mut trade_planner = V4Planner::default();
            trade_planner.add_trade(&trade, None).unwrap();

            assert_eq!(planner.actions, vec![0x07]);
            assert_eq!(
                planner.params[0],
                hex!("00000000000000000000000000000000000000000000000000000000000000200000000000000000000000006b175474e89094c44da98b954eedeac495271d0f00000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000100000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc20000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
            assert_eq!(planner.actions, trade_planner.actions);
            assert_eq!(planner.params[0], trade_planner.params[0]);
        }

        #[tokio::test]
        async fn completes_v4_exact_out_2_hop_swap() {
            let route = create_route!(DAI_USDC, USDC_WETH; DAI, WETH);
            let slippage_tolerance = Percent::new(5, 100);
            let trade = trade_from_route!(
                route,
                currency_amount!(WETH, ONE_ETHER),
                TradeType::ExactOutput
            );
            let mut planner = V4Planner::default();
            planner.add_trade(&trade, Some(slippage_tolerance)).unwrap();

            assert_eq!(planner.actions, vec![0x09]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc200000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000ea8d524a2a4ae240000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001000000000000000000000000006b175474e89094c44da98b954eedeac495271d0f0000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
        }

        #[tokio::test]
        async fn completes_v4_exact_out_2_hop_swap_route_path_output_different_than_route_output() {
            let route = create_route!(DAI_USDC, USDC_WETH; DAI, ETHER);
            let slippage_tolerance = Percent::new(5, 100);
            let trade = trade_from_route!(
                route,
                currency_amount!(ETHER, ONE_ETHER),
                TradeType::ExactOutput
            );
            let mut planner = V4Planner::default();
            planner.add_trade(&trade, Some(slippage_tolerance)).unwrap();

            assert_eq!(planner.actions, vec![0x09]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc200000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000ea8d524a2a4ae240000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000001000000000000000000000000006b175474e89094c44da98b954eedeac495271d0f0000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
        }

        #[tokio::test]
        async fn completes_v4_exact_in_2_hop_swap_route_path_input_different_than_route_input() {
            let route = create_route!(USDC_WETH, DAI_USDC; ETHER, DAI);
            let slippage_tolerance = Percent::new(5, 100);
            let trade = trade_from_route!(
                route,
                currency_amount!(ETHER, ONE_ETHER),
                TradeType::ExactInput
            );
            let mut planner = V4Planner::default();
            planner.add_trade(&trade, Some(slippage_tolerance)).unwrap();

            assert_eq!(planner.actions, vec![0x07]);
            assert_eq!(
                planner.params[0],
                hex!("0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc200000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000d23441c93fad7ca000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000100000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006b175474e89094c44da98b954eedeac495271d0f0000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000").to_vec()
            );
        }

        #[tokio::test]
        #[should_panic(expected = "ExactOut requires slippageTolerance")]
        async fn throws_error_if_adding_exact_out_trade_without_slippage_tolerance() {
            let route = create_route!(DAI_USDC, USDC_WETH; DAI, WETH);
            let trade = trade_from_route!(
                route,
                currency_amount!(WETH, ONE_ETHER),
                TradeType::ExactOutput
            );
            V4Planner::default().add_trade(&trade, None).unwrap();
        }

        #[tokio::test]
        #[should_panic(
            expected = "Only accepts Trades with 1 swap (must break swaps into individual trades)"
        )]
        async fn throws_error_if_adding_multiple_swaps_trade() {
            let slippage_tolerance = Percent::new(5, 100);
            let amount = currency_amount!(WETH, 1_000_000_000);
            let route1 = create_route!(DAI_USDC, USDC_WETH; DAI, WETH);
            let route2 = create_route!(DAI_WETH, DAI, WETH);
            let trade = Trade::from_routes(
                vec![(amount.clone(), route1), (amount, route2)],
                TradeType::ExactOutput,
            )
            .await
            .unwrap();
            V4Planner::default()
                .add_trade(&trade, Some(slippage_tolerance))
                .unwrap();
        }
    }
}
