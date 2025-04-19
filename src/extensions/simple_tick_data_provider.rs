use super::PoolManagerLens;
use alloy::{
    eips::BlockId,
    network::{Ethereum, Network},
    providers::Provider,
};
use alloy_primitives::{aliases::I24, Address, B256, U256};
use uniswap_v3_sdk::prelude::*;

#[derive(Clone, Debug)]
pub struct SimpleTickDataProvider<P, N = Ethereum, I = I24>
where
    P: Provider<N>,
    N: Network,
    I: TickIndex,
{
    pub lens: PoolManagerLens<P, N>,
    pub pool_id: B256,
    pub block_id: Option<BlockId>,
    _tick_index: core::marker::PhantomData<I>,
}

impl<P, N, I> SimpleTickDataProvider<P, N, I>
where
    P: Provider<N>,
    N: Network,
    I: TickIndex,
{
    #[inline]
    pub const fn new(
        manager: Address,
        pool_id: B256,
        provider: P,
        block_id: Option<BlockId>,
    ) -> Self {
        Self {
            lens: PoolManagerLens::new(manager, provider),
            pool_id,
            block_id,
            _tick_index: core::marker::PhantomData,
        }
    }
}

impl<P, N, I> TickBitMapProvider for SimpleTickDataProvider<P, N, I>
where
    P: Provider<N>,
    N: Network,
    I: TickIndex,
{
    type Index = I;

    #[inline]
    async fn get_word(&self, index: Self::Index) -> Result<U256, Error> {
        self.lens
            .get_tick_bitmap(self.pool_id, index, self.block_id)
            .await
    }
}

impl<P, N, I> TickDataProvider for SimpleTickDataProvider<P, N, I>
where
    P: Provider<N>,
    N: Network,
    I: TickIndex,
{
    type Index = I;

    #[inline]
    async fn get_tick(&self, index: Self::Index) -> Result<Tick<Self::Index>, Error> {
        let (liquidity_gross, liquidity_net) = self
            .lens
            .get_tick_liquidity(self.pool_id, index, self.block_id)
            .await?;
        Ok(Tick {
            index,
            liquidity_gross,
            liquidity_net,
        })
    }

    #[inline]
    async fn next_initialized_tick_within_one_word(
        &self,
        tick: Self::Index,
        lte: bool,
        tick_spacing: Self::Index,
    ) -> Result<(Self::Index, bool), Error> {
        TickBitMapProvider::next_initialized_tick_within_one_word(self, tick, lte, tick_spacing)
            .await
    }
}
