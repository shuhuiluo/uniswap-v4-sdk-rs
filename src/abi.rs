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
        uint160 sqrtPriceLimitX96;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactInParams {
        address currencyIn;
        PathKey[] path;
        uint128 amountIn;
        uint128 amountOutMinimum;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactOutSingleParams {
        PoolKey poolKey;
        bool zeroForOne;
        uint128 amountOut;
        uint128 amountInMaximum;
        uint160 sqrtPriceLimitX96;
        bytes hookData;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct SwapExactOutParams {
        address currencyOut;
        PathKey[] path;
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
    struct SettleTakePairParams {
        address settleCurrency;
        address takeCurrency;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct CloseCurrencyParams {
        address currency;
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
}
