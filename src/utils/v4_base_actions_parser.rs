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
        ActionsParams::abi_decode(calldata.iter().as_slice(), true)?;
    Ok(V4RouterCall {
        actions: zip(actions, params)
            .map(|(command, data)| Actions::abi_decode(command, &data))
            .collect::<Result<Vec<Actions>, Error>>()?,
    })
}
