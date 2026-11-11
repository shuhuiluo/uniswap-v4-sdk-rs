// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;
import {Test} from "forge-std/Test.sol";
import {Pool} from "v4-core/src/libraries/Pool.sol";
import {TickMath} from "v4-core/src/libraries/TickMath.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "v4-core/src/types/BalanceDelta.sol";

contract Harness {
    using Pool for Pool.State;
    using BalanceDeltaLibrary for BalanceDelta;
    Pool.State private pool;

    function run(
        uint24 lp,
        uint24 protocol,
        int24 lower,
        int24 upper,
        uint128 liquidity,
        bool direction,
        int256 amount,
        int24 limit
    ) external returns (int128, int128, uint160, int24, uint128, uint24) {
        pool.initialize(uint160(1 << 96), lp);
        pool.setProtocolFee(protocol);
        if (lower < upper && liquidity > 0) {
            pool.modifyLiquidity(
                Pool.ModifyLiquidityParams({
                    owner: address(this),
                    tickLower: lower,
                    tickUpper: upper,
                    liquidityDelta: int128(liquidity),
                    tickSpacing: 10,
                    salt: bytes32(0)
                })
            );
        }
        (BalanceDelta delta,, uint24 fee, Pool.SwapResult memory result) = pool.swap(
            Pool.SwapParams({
                amountSpecified: amount,
                tickSpacing: 10,
                zeroForOne: direction,
                sqrtPriceLimitX96: TickMath.getSqrtPriceAtTick(limit),
                lpFeeOverride: 0
            })
        );
        return (delta.amount0(), delta.amount1(), result.sqrtPriceX96, result.tick, result.liquidity, fee);
    }
}

contract OracleTest is Test {
    uint256 private next;
    string private output;

    function record(
        uint24 lp,
        uint24 protocol,
        int24 lower,
        int24 upper,
        uint128 liquidity,
        bool direction,
        bool exactIn,
        uint256 amount,
        int24 limit
    ) internal {
        string memory key = string.concat("case_", vm.toString(next++));
        vm.serializeString(key, "lp_fee", vm.toString(lp));
        vm.serializeString(key, "protocol_fee", vm.toString(protocol));
        vm.serializeString(key, "lower", vm.toString(int256(lower)));
        vm.serializeString(key, "upper", vm.toString(int256(upper)));
        vm.serializeString(key, "range_liquidity", vm.toString(liquidity));
        vm.serializeBool(key, "zero_for_one", direction);
        vm.serializeBool(key, "exact_in", exactIn);
        vm.serializeString(key, "amount", vm.toString(amount));
        string memory row = vm.serializeString(key, "limit_tick", vm.toString(int256(limit)));
        Harness harness = new Harness();
        try harness.run(
            lp, protocol, lower, upper, liquidity, direction, exactIn ? -int256(amount) : int256(amount), limit
        ) returns (
            int128 a0, int128 a1, uint160 price, int24 tick, uint128 active, uint24 fee
        ) {
            vm.serializeString(key, "amount0", vm.toString(int256(a0)));
            vm.serializeString(key, "amount1", vm.toString(int256(a1)));
            vm.serializeString(key, "sqrt_price", vm.toString(price));
            vm.serializeString(key, "tick", vm.toString(int256(tick)));
            vm.serializeString(key, "liquidity", vm.toString(active));
            vm.serializeString(key, "swap_fee", vm.toString(fee));
            row = vm.serializeBool(key, "reverted", false);
        } catch (bytes memory reason) {
            vm.serializeString(key, "revert_data", vm.toString(reason));
            row = vm.serializeBool(key, "reverted", true);
        }
        output = vm.serializeString("all", key, row);
    }

    function testExport() public {
        output = vm.serializeString("all", "core_revision", "bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6");
        uint24[4] memory fees = [uint24(0), uint24(500), uint24(3000), uint24(1000000)];
        for (uint256 f; f < fees.length; f++) {
            for (uint256 p; p < 2; p++) {
                for (uint256 d; d < 2; d++) {
                    for (uint256 e; e < 2; e++) {
                        record(
                            fees[f],
                            p == 0 ? 0 : uint24(1000 | (500 << 12)),
                            -120,
                            120,
                            1e24,
                            d == 0,
                            e == 0,
                            1e15,
                            d == 0 ? int24(-100) : int24(100)
                        );
                    }
                }
            }
        }
        record(3000, 1000, -10, 10, 1e20, true, true, 1e18, -10);
        record(3000, uint24(500 << 12), -10, 10, 1e20, false, true, 1e18, 10);
        record(1000000, 0, -120, -60, 1e20, true, true, 1e15, -30);
        record(1000000, 0, -120, -60, 1e20, true, true, 1e15, -90);
        record(1000000, 0, 60, 120, 1e20, false, true, 1e15, 30);
        record(1000000, 0, 60, 120, 1e20, false, true, 1e15, 90);
        record(1000000, 0, 0, 0, 0, true, true, 1e15, -30);
        record(1000000, 0, 0, 0, 0, false, true, 1e15, 30);
        vm.writeJson(output, "./vectors.json");
        emit log_named_uint("exported cases", next);
    }
}
