use crate::prelude::{Actions, ActionsParams, Error};
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
        ActionsParams::abi_decode(calldata.iter().as_slice(), true)?;
    let mut res = V4RouterCall {
        actions: Vec::with_capacity(actions.len()),
    };
    for (command, data) in zip(actions, params) {
        res.actions.push(Actions::abi_decode(command, &data)?);
    }
    Ok(res)
}
