use crate::prelude::{Actions, ActionsParams, Error, URVersion};
use alloc::vec::Vec;
use alloy_primitives::Bytes;
use alloy_sol_types::SolType;
use core::iter::zip;

#[derive(Clone, Debug, PartialEq)]
pub struct V4RouterCall {
    pub actions: Vec<Actions>,
}

#[inline]
pub fn parse_calldata(calldata: &Bytes, version: URVersion) -> Result<V4RouterCall, Error> {
    let ActionsParams { actions, params } =
        ActionsParams::abi_decode_params_validate(calldata.iter().as_slice())?;
    if actions.len() != params.len() {
        return Err(Error::MismatchedActionParams);
    }
    Ok(V4RouterCall {
        actions: zip(actions, params)
            .map(|(command, data)| Actions::abi_decode(command, &data, version))
            .collect::<Result<Vec<Actions>, Error>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_route, prelude::*, tests::*};
    use alloy_primitives::{Address, U256, address, uint};
    use alloy_sol_types::SolValue;
    use once_cell::sync::Lazy;
    use uniswap_v3_sdk::prelude::FeeAmount;

    const ADDRESS_ONE: Address = address!("0000000000000000000000000000000000000001");
    const ADDRESS_TWO: Address = address!("0000000000000000000000000000000000000002");
    const AMOUNT: U256 = uint!(1_000_000_000_000_000_000_U256);

    static USDC_WETH: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            USDC.clone().into(),
            WETH.clone().into(),
            FeeAmount::MEDIUM.into(),
            10,
            Address::ZERO,
            *SQRT_PRICE_1_1,
            0,
        )
        .unwrap()
    });

    #[test]
    fn test_parse_calldata() {
        let route = create_route!(DAI_USDC, USDC_WETH; DAI, WETH);
        let tests: Vec<(Actions, URVersion)> = vec![
            (
                Actions::SWEEP(SweepParams {
                    currency: ADDRESS_ONE,
                    recipient: ADDRESS_TWO,
                }),
                URVersion::default(),
            ),
            (Actions::CLOSE_CURRENCY(ADDRESS_ONE), URVersion::default()),
            (
                Actions::TAKE_PAIR(TakePairParams {
                    currency0: ADDRESS_ONE,
                    currency1: ADDRESS_TWO,
                    recipient: ADDRESS_ONE,
                }),
                URVersion::default(),
            ),
            (
                Actions::TAKE_PORTION(TakePortionParams {
                    currency: ADDRESS_ONE,
                    recipient: ADDRESS_TWO,
                    bips: AMOUNT,
                }),
                URVersion::default(),
            ),
            (
                Actions::TAKE_ALL(TakeAllParams {
                    currency: ADDRESS_ONE,
                    minAmount: AMOUNT,
                }),
                URVersion::default(),
            ),
            (
                Actions::TAKE(TakeParams {
                    currency: ADDRESS_ONE,
                    recipient: ADDRESS_TWO,
                    amount: AMOUNT,
                }),
                URVersion::default(),
            ),
            (
                Actions::SETTLE_PAIR(SettlePairParams {
                    currency0: ADDRESS_ONE,
                    currency1: ADDRESS_TWO,
                }),
                URVersion::default(),
            ),
            (
                Actions::SETTLE(SettleParams {
                    currency: ADDRESS_ONE,
                    amount: AMOUNT,
                    payerIsUser: true,
                }),
                URVersion::default(),
            ),
            (
                Actions::SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams {
                    poolKey: USDC_WETH.pool_key.clone(),
                    zeroForOne: true,
                    amountIn: AMOUNT.try_into().unwrap(),
                    amountOutMinimum: AMOUNT.try_into().unwrap(),
                    hookData: Bytes::default(),
                }),
                URVersion::default(),
            ),
            (
                Actions::SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams {
                    poolKey: USDC_WETH.pool_key.clone(),
                    zeroForOne: true,
                    amountOut: AMOUNT.try_into().unwrap(),
                    amountInMaximum: AMOUNT.try_into().unwrap(),
                    hookData: Bytes::default(),
                }),
                URVersion::default(),
            ),
            (
                Actions::SWAP_EXACT_IN(
                    SwapExactInParams {
                        currencyIn: DAI.address,
                        path: encode_route_to_path(&route, false),
                        amountIn: AMOUNT.try_into().unwrap(),
                        amountOutMinimum: AMOUNT.try_into().unwrap(),
                    }
                    .into(),
                ),
                URVersion::V2_0,
            ),
            (
                Actions::SWAP_EXACT_OUT(
                    SwapExactOutParams {
                        currencyOut: DAI.address,
                        path: encode_route_to_path(&route, true),
                        amountOut: AMOUNT.try_into().unwrap(),
                        amountInMaximum: AMOUNT.try_into().unwrap(),
                    }
                    .into(),
                ),
                URVersion::V2_0,
            ),
            (
                Actions::SWAP_EXACT_IN(
                    SwapExactInParamsV2_1 {
                        currencyIn: DAI.address,
                        path: encode_route_to_path(&route, false),
                        maxHopSlippage: vec![uint!(10000_U256), uint!(20000_U256)],
                        amountIn: AMOUNT.try_into().unwrap(),
                        amountOutMinimum: 0,
                    }
                    .into(),
                ),
                URVersion::V2_1,
            ),
            (
                Actions::SWAP_EXACT_OUT(
                    SwapExactOutParamsV2_1 {
                        currencyOut: WETH.address,
                        path: encode_route_to_path(&route, true),
                        maxHopSlippage: vec![uint!(15000_U256), uint!(25000_U256)],
                        amountOut: AMOUNT.try_into().unwrap(),
                        amountInMaximum: AMOUNT.try_into().unwrap(),
                    }
                    .into(),
                ),
                URVersion::V2_1,
            ),
            (
                Actions::SWAP_EXACT_IN(
                    SwapExactInParamsV2_1 {
                        currencyIn: DAI.address,
                        path: encode_route_to_path(&route, false),
                        maxHopSlippage: Vec::new(),
                        amountIn: AMOUNT.try_into().unwrap(),
                        amountOutMinimum: 0,
                    }
                    .into(),
                ),
                URVersion::V2_1,
            ),
        ];

        for (test, version) in tests {
            let mut planner = V4Planner::default();
            planner.add_action(&test);
            let calldata = planner.finalize();
            let result = parse_calldata(&calldata, version).unwrap();
            assert_eq!(result.actions, vec![test]);
        }
    }

    #[test]
    fn test_parse_calldata_rejects_mismatched_action_params() {
        let calldata: Bytes = ActionsParams {
            actions: vec![0x12, 0x16].into(),
            params: vec![Address::ZERO.abi_encode().into()],
        }
        .abi_encode_params()
        .into();

        assert!(matches!(
            parse_calldata(&calldata, URVersion::default()).unwrap_err(),
            Error::MismatchedActionParams
        ));
    }
}
