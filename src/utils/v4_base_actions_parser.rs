use crate::prelude::{Actions, ActionsParams, Error};
use alloc::vec::Vec;
use alloy_primitives::Bytes;
use alloy_sol_types::SolType;
use core::iter::zip;

#[derive(Clone, Debug, PartialEq)]
pub struct V4RouterCall {
    pub actions: Vec<Actions>,
}

#[inline]
pub fn parse_calldata(calldata: &Bytes) -> Result<V4RouterCall, Error> {
    let ActionsParams { actions, params } =
        ActionsParams::abi_decode_params_validate(calldata.iter().as_slice())?;
    Ok(V4RouterCall {
        actions: zip(actions, params)
            .map(|(command, data)| Actions::abi_decode(command, &data))
            .collect::<Result<Vec<Actions>, Error>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_route, prelude::*, tests::*};
    use alloy_primitives::{address, uint, Address, U256};
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
        let tests: Vec<Actions> = vec![
            Actions::SWEEP(SweepParams {
                currency: ADDRESS_ONE,
                recipient: ADDRESS_TWO,
            }),
            Actions::CLOSE_CURRENCY(ADDRESS_ONE),
            Actions::TAKE_PAIR(TakePairParams {
                currency0: ADDRESS_ONE,
                currency1: ADDRESS_TWO,
                recipient: ADDRESS_ONE,
            }),
            Actions::TAKE_PORTION(TakePortionParams {
                currency: ADDRESS_ONE,
                recipient: ADDRESS_TWO,
                bips: AMOUNT,
            }),
            Actions::TAKE_ALL(TakeAllParams {
                currency: ADDRESS_ONE,
                minAmount: AMOUNT,
            }),
            Actions::TAKE(TakeParams {
                currency: ADDRESS_ONE,
                recipient: ADDRESS_TWO,
                amount: AMOUNT,
            }),
            Actions::SETTLE_PAIR(SettlePairParams {
                currency0: ADDRESS_ONE,
                currency1: ADDRESS_TWO,
            }),
            Actions::SETTLE(SettleParams {
                currency: ADDRESS_ONE,
                amount: AMOUNT,
                payerIsUser: true,
            }),
            Actions::SWAP_EXACT_IN_SINGLE(SwapExactInSingleParams {
                poolKey: USDC_WETH.pool_key.clone(),
                zeroForOne: true,
                amountIn: AMOUNT.try_into().unwrap(),
                amountOutMinimum: AMOUNT.try_into().unwrap(),
                hookData: Bytes::default(),
            }),
            Actions::SWAP_EXACT_OUT_SINGLE(SwapExactOutSingleParams {
                poolKey: USDC_WETH.pool_key.clone(),
                zeroForOne: true,
                amountOut: AMOUNT.try_into().unwrap(),
                amountInMaximum: AMOUNT.try_into().unwrap(),
                hookData: Bytes::default(),
            }),
            Actions::SWAP_EXACT_IN(SwapExactInParams {
                currencyIn: DAI.address,
                path: encode_route_to_path(&create_route!(DAI_USDC, USDC_WETH; DAI, WETH), false),
                amountIn: AMOUNT.try_into().unwrap(),
                amountOutMinimum: AMOUNT.try_into().unwrap(),
            }),
            Actions::SWAP_EXACT_OUT(SwapExactOutParams {
                currencyOut: DAI.address,
                path: encode_route_to_path(&create_route!(DAI_USDC, USDC_WETH; DAI, WETH), false),
                amountOut: AMOUNT.try_into().unwrap(),
                amountInMaximum: AMOUNT.try_into().unwrap(),
            }),
        ];

        for test in tests {
            let mut planner = V4Planner::default();
            planner.add_action(&test);
            let calldata = planner.finalize();
            let result = parse_calldata(&calldata).unwrap();
            assert_eq!(result.actions, vec![test]);
        }
    }
}
