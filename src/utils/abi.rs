use alloy_sol_types::sol;

sol! {
    #[derive(Debug, Default, PartialEq, Eq)]
    struct PermitDetails {
        address token;
        uint160 amount;
        uint48 expiration;
        uint48 nonce;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct PermitSingle {
        PermitDetails details;
        address spender;
        uint256 sigDeadline;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct PermitBatch {
        
        address spender;
        PermitDetails[] details;
        uint256 sigDeadline;
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct PoolKey {
        address currency0;
        address currency1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    // Event types
    // #[derive(Debug, PartialEq, Eq)]
    // event Approval {
    //     #[indexed]
    //     address owner;
    //     #[indexed]
    //     address spender;
    //     #[indexed]
    //     uint256 id;
    // }

    // #[derive(Debug, PartialEq, Eq)]
    // event ApprovalForAll {
    //     #[indexed]
    //     address owner;
    //     #[indexed]
    //     address operator;
    //     bool approved;
    // }

    // #[derive(Debug, PartialEq, Eq)]
    // event Subscription {
    //     #[indexed]
    //     uint256 tokenId;
    //     #[indexed]
    //     address subscriber;
    // }

    // #[derive(Debug, PartialEq, Eq)]
    // event Transfer {
    //     #[indexed]
    //     address from;
    //     #[indexed]
    //     address to;
    //     #[indexed]
    //     uint256 id;
    // }

    // #[derive(Debug, PartialEq, Eq)]
    // event Unsubscription {
    //     #[indexed]
    //     uint256 tokenId;
    //     #[indexed]
    //     address subscriber;
    // }

    // Error types
    #[derive(Debug, PartialEq, Eq)]
    error AlreadySubscribed(uint256 tokenId, address subscriber);

    #[derive(Debug, PartialEq, Eq)]
    error ContractLocked();

    #[derive(Debug, PartialEq, Eq)]
    error DeadlinePassed(uint256 deadline);

    #[derive(Debug, PartialEq, Eq)]
    error DeltaNotNegative(address currency);

    #[derive(Debug, PartialEq, Eq)]
    error DeltaNotPositive(address currency);

    #[derive(Debug, PartialEq, Eq)]
    error GasLimitTooLow();

    #[derive(Debug, PartialEq, Eq)]
    error InputLengthMismatch();

    #[derive(Debug, PartialEq, Eq)]
    error InvalidContractSignature();

    #[derive(Debug, PartialEq, Eq)]
    error InvalidSignature();

    #[derive(Debug, PartialEq, Eq)]
    error InvalidSignatureLength();

    #[derive(Debug, PartialEq, Eq)]
    error InvalidSigner();

    #[derive(Debug, PartialEq, Eq)]
    error MaximumAmountExceeded(uint128 maximumAmount, uint128 amountRequested);

    #[derive(Debug, PartialEq, Eq)]
    error MinimumAmountInsufficient(uint128 minimumAmount, uint128 amountReceived);

    #[derive(Debug, PartialEq, Eq)]
    error NoCodeSubscriber();

    #[derive(Debug, PartialEq, Eq)]
    error NoSelfPermit();

    #[derive(Debug, PartialEq, Eq)]
    error NonceAlreadyUsed();

    #[derive(Debug, PartialEq, Eq)]
    error NotApproved(address caller);

    #[derive(Debug, PartialEq, Eq)]
    error NotPoolManager();

    #[derive(Debug, PartialEq, Eq)]
    error NotSubscribed();

    #[derive(Debug, PartialEq, Eq)]
    error SignatureDeadlineExpired();

    #[derive(Debug, PartialEq, Eq)]
    error Unauthorized();

    #[derive(Debug, PartialEq, Eq)]
    error UnsupportedAction(uint256 action);

    #[derive(Debug, PartialEq, Eq)]
    error Wrap__ModifyLiquidityNotificationReverted(address subscriber, bytes reason);

    #[derive(Debug, PartialEq, Eq)]
    error Wrap__SubscriptionReverted(address subscriber, bytes reason);

    #[derive(Debug, PartialEq, Eq)]
    error Wrap__TransferNotificationReverted(address subscriber, bytes reason);
}