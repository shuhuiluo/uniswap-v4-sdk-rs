use alloy_sol_types::sol;

sol! {
    #[derive(Debug, Default, PartialEq, Eq)]
    struct PoolKey {
        address currency0;
        address currency1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct PathKey {
        address intermediateCurrency;
        uint256 fee;
        int24 tickSpacing;
        address hooks;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct IncreaseLiquidityParams {
        uint256 tokenId;
        uint256 liquidity;
        uint128 amount0Max;
        uint128 amount1Max;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct DecreaseLiquidityParams {
        uint256 tokenId;
        uint256 liquidity;
        uint128 amount0Min;
        uint128 amount1Min;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct MintPositionParams {
        PoolKey poolKey;
        int24 tickLower;
        int24 tickUpper;
        uint256 liquidity;
        uint128 amount0Max;
        uint128 amount1Max;
        address owner;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct BurnPositionParams {
        uint256 tokenId;
        uint128 amount0Min;
        uint128 amount1Min;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactInSingleParams {
        PoolKey poolKey;
        bool zeroForOne;
        uint128 amountIn;
        uint128 amountOutMinimum;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactInParams {
        address currencyIn;
        PathKey[] path;
        uint256[] maxHopSlippage;
        uint128 amountIn;
        uint128 amountOutMinimum;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactOutSingleParams {
        PoolKey poolKey;
        bool zeroForOne;
        uint128 amountOut;
        uint128 amountInMaximum;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactOutParams {
        address currencyOut;
        PathKey[] path;
        uint256[] maxHopSlippage;
        uint128 amountOut;
        uint128 amountInMaximum;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SettleParams {
        address currency;
        uint256 amount;
        bool payerIsUser;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SettleAllParams {
        address currency;
        uint256 maxAmount;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SettlePairParams {
        address currency0;
        address currency1;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct TakeParams {
        address currency;
        address recipient;
        uint256 amount;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct TakeAllParams {
        address currency;
        uint256 minAmount;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct TakePortionParams {
        address currency;
        address recipient;
        uint256 bips;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct TakePairParams {
        address currency0;
        address currency1;
        address recipient;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SweepParams {
        address currency;
        address recipient;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct ActionsParams {
        bytes actions;
        bytes[] params;
    }

    interface IAllowanceTransfer {
        /// @notice The permit data for a token
        #[derive(Debug, Default, PartialEq, Eq)]
        struct PermitDetails {
            // ERC20 token address
            address token;
            // the maximum amount allowed to spend
            uint160 amount;
            // timestamp at which a spender's token allowances become invalid
            uint48 expiration;
            // an incrementing value indexed per owner,token,and spender for each signature
            uint48 nonce;
        }

        /// @notice The permit message signed for a single token allowance
        #[derive(Debug, Default, PartialEq, Eq)]
        struct PermitSingle {
            // the permit data for a single token allowance
            PermitDetails details;
            // address permissioned on the allowed tokens
            address spender;
            // deadline on the permit signature
            uint256 sigDeadline;
        }

        /// @notice The permit message signed for multiple token allowances
        #[derive(Debug, Default, PartialEq, Eq)]
        struct PermitBatch {
            // the permit data for multiple token allowances
            PermitDetails[] details;
            // address permissioned on the allowed tokens
            address spender;
            // deadline on the permit signature
            uint256 sigDeadline;
        }

        function approve(address token, address spender, uint160 amount, uint48 expiration) external;
    }

    interface IPositionManager {
        function initializePool(PoolKey calldata key, uint160 sqrtPriceX96) external payable returns (int24);

        function modifyLiquidities(bytes calldata unlockData, uint256 deadline) external payable;

        function permitBatch(address owner, IAllowanceTransfer.PermitBatch calldata _permitBatch, bytes calldata signature)
            external
            payable
            returns (bytes memory err);

        function permit(address spender, uint256 tokenId, uint256 deadline, uint256 nonce, bytes calldata signature)
            external
            payable;
    }

    #[cfg(feature = "extensions")]
    #[sol(rpc)]
    interface IPositionManagerView {
        type PositionInfo is uint256;

        function poolManager() external view returns (address);

        function getPositionLiquidity(uint256 tokenId) external view returns (uint128 liquidity);

        function getPoolAndPositionInfo(uint256 tokenId) external view returns (PoolKey memory, PositionInfo);
    }
}

#[cfg(feature = "extensions")]
alloy::sol! {
    #[sol(rpc)]
    interface IExtsload {
        function extsload(bytes32 slot) external view returns (bytes32 value);
        function extsload(bytes32 startSlot, uint256 nSlots) external view returns (bytes32[] memory values);
        function extsload(bytes32[] calldata slots) external view returns (bytes32[] memory values);
    }
}

#[cfg(feature = "extensions")]
alloy::sol! {
    type PoolId is bytes32;

    event ModifyLiquidity(
        PoolId indexed id, address indexed sender, int24 tickLower, int24 tickUpper, int256 liquidityDelta, bytes32 salt
    );

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    #[sol(rpc)]
    #[derive(Debug)]
    interface IStateView {
        function getSlot0(PoolId poolId)
            external
            view
            returns (uint160 sqrtPriceX96, int24 tick, uint24 protocolFee, uint24 lpFee);
        function getTickInfo(PoolId poolId, int24 tick)
            external
            view
            returns (uint128 liquidityGross, int128 liquidityNet, uint256 feeGrowthOutside0X128, uint256 feeGrowthOutside1X128);
        function getTickLiquidity(PoolId poolId, int24 tick)
            external
            view
            returns (uint128 liquidityGross, int128 liquidityNet);
        function getTickFeeGrowthOutside(PoolId poolId, int24 tick)
            external
            view
            returns (uint256 feeGrowthOutside0X128, uint256 feeGrowthOutside1X128);
        function getFeeGrowthGlobals(PoolId poolId)
            external
            view
            returns (uint256 feeGrowthGlobal0, uint256 feeGrowthGlobal1);
        function getLiquidity(PoolId poolId) external view returns (uint128 liquidity);
        function getTickBitmap(PoolId poolId, int16 tick) external view returns (uint256 tickBitmap);
        function getPositionInfo(PoolId poolId, address owner, int24 tickLower, int24 tickUpper, bytes32 salt)
            external
            view
            returns (uint128 liquidity, uint256 feeGrowthInside0LastX128, uint256 feeGrowthInside1LastX128);
        function getPositionInfo(PoolId poolId, bytes32 positionId)
            external
            view
            returns (uint128 liquidity, uint256 feeGrowthInside0LastX128, uint256 feeGrowthInside1LastX128);
        function getPositionLiquidity(PoolId poolId, bytes32 positionId) external view returns (uint128 liquidity);
        function getFeeGrowthInside(PoolId poolId, int24 tickLower, int24 tickUpper)
            external
            view
            returns (uint256 feeGrowthInside0X128, uint256 feeGrowthInside1X128);
    }
}
