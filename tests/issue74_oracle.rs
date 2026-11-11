//! Independent v4-core results, exercised through the public quote API.
use alloy_primitives::{Address, I256, U160, U256, address, aliases::U24};
use serde_json::Value;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;
use uniswap_v4_sdk::prelude::{DYANMIC_FEE_FLAG, Error, Pool};

// Vec's upstream provider requires a nonempty list. Model an empty bitmap explicitly.
#[derive(Clone, Debug)]
struct OracleTicks(Vec<Tick>);
impl TickDataProvider for OracleTicks {
    type Index = i32;
    async fn get_tick(&self, index: i32) -> Result<Tick, uniswap_v3_sdk::error::Error> {
        self.0.get_tick(index).await
    }
    async fn next_initialized_tick_within_one_word(
        &self,
        tick: i32,
        lte: bool,
        spacing: i32,
    ) -> Result<(i32, bool), uniswap_v3_sdk::error::Error> {
        if !self.0.is_empty() {
            return self
                .0
                .next_initialized_tick_within_one_word(tick, lte, spacing)
                .await;
        }
        let compressed = tick.div_euclid(spacing);
        Ok((
            if lte {
                (compressed >> 8) << 8
            } else {
                (((compressed + 1) >> 8) << 8) + 255
            } * spacing,
            false,
        ))
    }
}
fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap()
}
fn number<T: core::str::FromStr>(row: &Value, key: &str) -> T
where
    T::Err: core::fmt::Debug,
{
    text(row, key).parse().unwrap()
}

#[tokio::test]
async fn quotes_match_pinned_v4_core() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/v4_core.json")).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 40);
    let token0 = token!(1, "0000000000000000000000000000000000000001", 18);
    let token1 = token!(1, "0000000000000000000000000000000000000002", 18);
    for row in fixture["cases"].as_array().unwrap() {
        let id = text(row, "id");
        let lower: i32 = number(row, "lower");
        let upper: i32 = number(row, "upper");
        let range_liquidity: u128 = number(row, "range_liquidity");
        let ticks = if range_liquidity == 0 {
            vec![]
        } else {
            vec![
                Tick {
                    index: lower,
                    liquidity_net: range_liquidity as i128,
                    liquidity_gross: range_liquidity,
                },
                Tick {
                    index: upper,
                    liquidity_net: -(range_liquidity as i128),
                    liquidity_gross: range_liquidity,
                },
            ]
        };
        let initial_liquidity = if lower <= 0 && 0 < upper {
            range_liquidity
        } else {
            0
        };
        let lp_fee: U24 = number(row, "lp_fee");
        let protocol_fee: U24 = number(row, "protocol_fee");
        let zero_for_one = row["zero_for_one"].as_bool().unwrap();
        let exact_in = row["exact_in"].as_bool().unwrap();
        for dynamic in [false, true] {
            let pool = Pool::new_with_tick_data_provider(
                token0.clone().into(),
                token1.clone().into(),
                if dynamic { DYANMIC_FEE_FLAG } else { lp_fee },
                10,
                if dynamic {
                    address!("0100000000000000000000000000000000000000")
                } else {
                    Address::ZERO
                },
                fixture["initial_sqrt_price"]
                    .as_str()
                    .unwrap()
                    .parse::<U160>()
                    .unwrap(),
                initial_liquidity,
                OracleTicks(ticks.clone()),
            )
            .unwrap()
            .with_fees(lp_fee, protocol_fee)
            .unwrap();
            if !row["reverted"].as_bool().unwrap() {
                assert_eq!(
                    pool.swap_fee(zero_for_one).unwrap(),
                    number::<U24>(row, "swap_fee"),
                    "{id}"
                );
            }
            let input = if zero_for_one {
                token0.clone()
            } else {
                token1.clone()
            };
            let output = if zero_for_one {
                token1.clone()
            } else {
                token0.clone()
            };
            let amount = number::<U256>(row, "amount").to_big_int();
            let limit =
                Some(get_sqrt_ratio_at_tick(number::<i32>(row, "limit_tick").to_i24()).unwrap());
            let result = if exact_in {
                pool.get_output_amount(
                    &CurrencyAmount::from_raw_amount(input, amount).unwrap(),
                    limit,
                )
                .await
            } else {
                pool.get_input_amount(
                    &CurrencyAmount::from_raw_amount(output, amount).unwrap(),
                    limit,
                )
                .await
            };
            if row["reverted"].as_bool().unwrap() {
                assert!(
                    matches!(result, Err(Error::InvalidFeeForExactOutput)),
                    "{id}"
                );
                continue;
            }
            let (amount, quoted) =
                result.unwrap_or_else(|error| panic!("{id}, dynamic={dynamic}: {error}"));
            let expected = if exact_in {
                number::<I256>(row, if zero_for_one { "amount1" } else { "amount0" }).to_big_int()
            } else {
                -number::<I256>(row, if zero_for_one { "amount0" } else { "amount1" }).to_big_int()
            };
            assert_eq!(amount.quotient(), expected, "{id}, dynamic={dynamic}");
            assert_eq!(
                quoted.sqrt_price_x96,
                number::<U160>(row, "sqrt_price"),
                "{id}"
            );
            assert_eq!(quoted.tick_current, number::<i32>(row, "tick"), "{id}");
            assert_eq!(quoted.liquidity, number::<u128>(row, "liquidity"), "{id}");
            assert_eq!(quoted.pool_id, pool.pool_id, "{id}");
            assert_eq!(quoted.pool_key, pool.pool_key, "{id}");
            assert_eq!(quoted.fee, pool.fee, "{id}");
            assert_eq!(quoted.lp_fee, pool.lp_fee, "{id}");
            assert_eq!(quoted.protocol_fee, pool.protocol_fee, "{id}");
        }
    }
}

#[tokio::test]
async fn quote_can_continue_after_crossing_initialized_tick() {
    let token0 = token!(1, "0000000000000000000000000000000000000001", 18);
    let token1 = token!(1, "0000000000000000000000000000000000000002", 18);
    let liquidity = 100_000_000_000_000_000_000_u128;
    let pool = Pool::new_with_tick_data_provider(
        token0.clone().into(),
        token1.clone().into(),
        U24::from(3000),
        10,
        Address::ZERO,
        get_sqrt_ratio_at_tick(0_i32.to_i24()).unwrap(),
        liquidity,
        OracleTicks(vec![
            Tick {
                index: -10,
                liquidity_net: liquidity as i128,
                liquidity_gross: liquidity,
            },
            Tick {
                index: 10,
                liquidity_net: -(liquidity as i128),
                liquidity_gross: liquidity,
            },
        ]),
    )
    .unwrap();
    let amount =
        CurrencyAmount::from_raw_amount(token0.clone(), 1_000_000_000_000_000_000_i64).unwrap();
    let (_, crossed) = pool
        .get_output_amount(
            &amount,
            Some(get_sqrt_ratio_at_tick((-10_i32).to_i24()).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(crossed.tick_current, -11);
    assert_eq!(crossed.liquidity, 0);
    let (continued, _) = crossed
        .get_output_amount(
            &amount,
            Some(get_sqrt_ratio_at_tick((-20_i32).to_i24()).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(continued.quotient(), BigInt::ZERO);
    let reversed_amount =
        CurrencyAmount::from_raw_amount(token1, 1_000_000_000_000_000_i64).unwrap();
    let (reversed, _) = crossed
        .get_output_amount(
            &reversed_amount,
            Some(get_sqrt_ratio_at_tick(0_i32.to_i24()).unwrap()),
        )
        .await
        .unwrap();
    assert!(reversed.quotient() > BigInt::ZERO);
}
