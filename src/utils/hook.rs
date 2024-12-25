use alloy_primitives::Address;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HookOptions {
    AfterRemoveLiquidityReturnsDelta = 0,
    AfterAddLiquidityReturnsDelta = 1,
    AfterSwapReturnsDelta = 2,
    BeforeSwapReturnsDelta = 3,
    AfterDonate = 4,
    BeforeDonate = 5,
    AfterSwap = 6,
    BeforeSwap = 7,
    AfterRemoveLiquidity = 8,
    BeforeRemoveLiquidity = 9,
    AfterAddLiquidity = 10,
    BeforeAddLiquidity = 11,
    AfterInitialize = 12,
    BeforeInitialize = 13,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HookPermissions {
    pub after_remove_liquidity_returns_delta: bool,
    pub after_add_liquidity_returns_delta: bool,
    pub after_swap_returns_delta: bool,
    pub before_swap_returns_delta: bool,
    pub after_donate: bool,
    pub before_donate: bool,
    pub after_swap: bool,
    pub before_swap: bool,
    pub after_remove_liquidity: bool,
    pub before_remove_liquidity: bool,
    pub after_add_liquidity: bool,
    pub before_add_liquidity: bool,
    pub after_initialize: bool,
    pub before_initialize: bool,
}

#[inline]
#[must_use]
pub const fn permissions(address: Address) -> HookPermissions {
    HookPermissions {
        before_initialize: has_permission(address, HookOptions::BeforeInitialize),
        after_initialize: has_permission(address, HookOptions::AfterInitialize),
        before_add_liquidity: has_permission(address, HookOptions::BeforeAddLiquidity),
        after_add_liquidity: has_permission(address, HookOptions::AfterAddLiquidity),
        before_remove_liquidity: has_permission(address, HookOptions::BeforeRemoveLiquidity),
        after_remove_liquidity: has_permission(address, HookOptions::AfterRemoveLiquidity),
        before_swap: has_permission(address, HookOptions::BeforeSwap),
        after_swap: has_permission(address, HookOptions::AfterSwap),
        before_donate: has_permission(address, HookOptions::BeforeDonate),
        after_donate: has_permission(address, HookOptions::AfterDonate),
        before_swap_returns_delta: has_permission(address, HookOptions::BeforeSwapReturnsDelta),
        after_swap_returns_delta: has_permission(address, HookOptions::AfterSwapReturnsDelta),
        after_add_liquidity_returns_delta: has_permission(
            address,
            HookOptions::AfterAddLiquidityReturnsDelta,
        ),
        after_remove_liquidity_returns_delta: has_permission(
            address,
            HookOptions::AfterRemoveLiquidityReturnsDelta,
        ),
    }
}

#[inline]
#[must_use]
pub const fn has_permission(address: Address, hook_option: HookOptions) -> bool {
    let mask = (address.0 .0[18] as u64) << 8 | (address.0 .0[19] as u64);
    let hook_flag_index = hook_option as u64;
    mask & (1 << hook_flag_index) != 0
}

#[inline]
#[must_use]
pub const fn has_initialize_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeInitialize)
        || has_permission(address, HookOptions::AfterInitialize)
}

#[inline]
#[must_use]
pub const fn has_liquidity_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeAddLiquidity)
        || has_permission(address, HookOptions::AfterAddLiquidity)
        || has_permission(address, HookOptions::BeforeRemoveLiquidity)
        || has_permission(address, HookOptions::AfterRemoveLiquidity)
}

#[inline]
#[must_use]
pub const fn has_swap_permissions(address: Address) -> bool {
    // this implicitly encapsulates swap delta permissions
    has_permission(address, HookOptions::BeforeSwap)
        || has_permission(address, HookOptions::AfterSwap)
}

#[inline]
#[must_use]
pub const fn has_donate_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeDonate)
        || has_permission(address, HookOptions::AfterDonate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, U160};
    use once_cell::sync::Lazy;

    fn construct_hook_address(hook_options: Vec<HookOptions>) -> Address {
        let mut hook_flags = U160::ZERO;
        let one = U160::from_limbs([1, 0, 0]);
        for hook_option in hook_options {
            hook_flags |= one << (hook_option as u8);
        }
        Address::from(hook_flags)
    }

    const ALL_HOOKS_ADDRESS: Address = address!("0000000000000000000000000000000000003fff");
    const EMPTY_HOOK_ADDRESS: Address = Address::ZERO;
    static HOOK_BEFORE_INITIALIZE: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeInitialize]));
    static HOOK_AFTER_INITIALIZE: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterInitialize]));
    static HOOK_BEFORE_ADD_LIQUIDITY: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeAddLiquidity]));
    static HOOK_AFTER_ADD_LIQUIDITY: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterAddLiquidity]));
    static HOOK_BEFORE_REMOVE_LIQUIDITY: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeRemoveLiquidity]));
    static HOOK_AFTER_REMOVE_LIQUIDITY: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterRemoveLiquidity]));
    static HOOK_BEFORE_SWAP: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeSwap]));
    static HOOK_AFTER_SWAP: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterSwap]));
    static HOOK_BEFORE_DONATE: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeDonate]));
    static HOOK_AFTER_DONATE: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterDonate]));
    static HOOK_BEFORE_SWAP_RETURNS_DELTA: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::BeforeSwapReturnsDelta]));
    static HOOK_AFTER_SWAP_RETURNS_DELTA: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterSwapReturnsDelta]));
    static HOOK_AFTER_ADD_LIQUIDITY_RETURNS_DELTA: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterAddLiquidityReturnsDelta]));
    static HOOK_AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA: Lazy<Address> =
        Lazy::new(|| construct_hook_address(vec![HookOptions::AfterRemoveLiquidityReturnsDelta]));

    mod permissions {
        use super::*;

        #[test]
        fn before_initialize() {
            assert!(permissions(*HOOK_BEFORE_INITIALIZE).before_initialize);
            assert!(permissions(ALL_HOOKS_ADDRESS).before_initialize);
            assert!(!permissions(*HOOK_AFTER_INITIALIZE).before_initialize);
            assert!(!permissions(EMPTY_HOOK_ADDRESS).before_initialize);
        }

        #[test]
        fn after_initialize() {
            assert!(permissions(*HOOK_AFTER_INITIALIZE).after_initialize);
            assert!(permissions(ALL_HOOKS_ADDRESS).after_initialize);
            assert!(!permissions(*HOOK_BEFORE_INITIALIZE).after_initialize);
            assert!(
                !permissions(*HOOK_BEFORE_ADD_LIQUIDITY).after_initialize
            );
            assert!(!permissions(EMPTY_HOOK_ADDRESS).after_initialize);
        }

        #[test]
        fn before_add_liquidity() {
            assert!(
                permissions(*HOOK_BEFORE_ADD_LIQUIDITY).before_add_liquidity
            );
            assert!(permissions(ALL_HOOKS_ADDRESS).before_add_liquidity);
            assert!(
                !permissions(*HOOK_BEFORE_INITIALIZE).before_add_liquidity
            );
            assert!(
                !permissions(*HOOK_AFTER_ADD_LIQUIDITY).before_add_liquidity
            );
            assert!(!permissions(EMPTY_HOOK_ADDRESS).before_add_liquidity);
        }

        #[test]
        fn after_add_liquidity() {
            assert!(
                permissions(*HOOK_AFTER_ADD_LIQUIDITY).after_add_liquidity
            );
            assert!(permissions(ALL_HOOKS_ADDRESS).after_add_liquidity);
            assert!(
                !permissions(*HOOK_BEFORE_ADD_LIQUIDITY).after_add_liquidity
            );
            assert!(
                !permissions(*HOOK_BEFORE_REMOVE_LIQUIDITY).after_add_liquidity
            );
            assert!(!permissions(EMPTY_HOOK_ADDRESS).after_add_liquidity);
        }

        #[test]
        fn before_remove_liquidity() {
            assert!(
                permissions(*HOOK_BEFORE_REMOVE_LIQUIDITY).before_remove_liquidity
            );
            assert!(permissions(ALL_HOOKS_ADDRESS).before_remove_liquidity);
            assert!(
                !permissions(*HOOK_AFTER_ADD_LIQUIDITY).before_remove_liquidity
            );
            assert!(
                !permissions(*HOOK_AFTER_REMOVE_LIQUIDITY).before_remove_liquidity
            );
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).before_remove_liquidity
            );
        }

        #[test]
        fn after_remove_liquidity() {
            assert!(
                permissions(*HOOK_AFTER_REMOVE_LIQUIDITY).after_remove_liquidity
            );
            assert!(permissions(ALL_HOOKS_ADDRESS).after_remove_liquidity);
            assert!(
                !permissions(*HOOK_BEFORE_REMOVE_LIQUIDITY).after_remove_liquidity
            );
            assert!(!permissions(*HOOK_BEFORE_SWAP).after_remove_liquidity);
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).after_remove_liquidity
            );
        }

        #[test]
        fn before_swap() {
            assert!(permissions(*HOOK_BEFORE_SWAP).before_swap);
            assert!(permissions(ALL_HOOKS_ADDRESS).before_swap);
            assert!(!permissions(*HOOK_AFTER_REMOVE_LIQUIDITY).before_swap);
            assert!(!permissions(*HOOK_AFTER_SWAP).before_swap);
            assert!(!permissions(EMPTY_HOOK_ADDRESS).before_swap);
        }

        #[test]
        fn after_swap() {
            assert!(permissions(*HOOK_AFTER_SWAP).after_swap);
            assert!(permissions(ALL_HOOKS_ADDRESS).after_swap);
            assert!(!permissions(*HOOK_BEFORE_SWAP).after_swap);
            assert!(!permissions(*HOOK_BEFORE_DONATE).after_swap);
            assert!(!permissions(EMPTY_HOOK_ADDRESS).after_swap);
        }

        #[test]
        fn before_donate() {
            assert!(permissions(*HOOK_BEFORE_DONATE).before_donate);
            assert!(permissions(ALL_HOOKS_ADDRESS).before_donate);
            assert!(!permissions(*HOOK_AFTER_SWAP).before_donate);
            assert!(!permissions(*HOOK_AFTER_DONATE).before_donate);
            assert!(!permissions(EMPTY_HOOK_ADDRESS).before_donate);
        }

        #[test]
        fn after_donate() {
            assert!(permissions(*HOOK_AFTER_DONATE).after_donate);
            assert!(permissions(ALL_HOOKS_ADDRESS).after_donate);
            assert!(!permissions(*HOOK_BEFORE_DONATE).after_donate);
            assert!(
                !permissions(*HOOK_BEFORE_SWAP_RETURNS_DELTA).after_donate
            );
            assert!(!permissions(EMPTY_HOOK_ADDRESS).after_donate);
        }

        #[test]
        fn before_swap_returns_delta() {
            assert!(
                permissions(*HOOK_BEFORE_SWAP_RETURNS_DELTA).before_swap_returns_delta
            );
            assert!(
                permissions(ALL_HOOKS_ADDRESS).before_swap_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_DONATE).before_swap_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_SWAP_RETURNS_DELTA).before_swap_returns_delta
            );
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).before_swap_returns_delta
            );
        }

        #[test]
        fn after_swap_returns_delta() {
            assert!(
                permissions(*HOOK_AFTER_SWAP_RETURNS_DELTA).after_swap_returns_delta
            );
            assert!(
                permissions(ALL_HOOKS_ADDRESS).after_swap_returns_delta
            );
            assert!(
                !permissions(*HOOK_BEFORE_SWAP_RETURNS_DELTA).after_swap_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_ADD_LIQUIDITY_RETURNS_DELTA).after_swap_returns_delta
            );
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).after_swap_returns_delta
            );
        }

        #[test]
        fn after_add_liquidity_returns_delta() {
            assert!(
                permissions(*HOOK_AFTER_ADD_LIQUIDITY_RETURNS_DELTA)
                    .after_add_liquidity_returns_delta
            );
            assert!(
                permissions(ALL_HOOKS_ADDRESS).after_add_liquidity_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_SWAP_RETURNS_DELTA).after_add_liquidity_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA)
                    .after_add_liquidity_returns_delta
            );
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).after_add_liquidity_returns_delta
            );
        }

        #[test]
        fn after_remove_liquidity_returns_delta() {
            assert!(
                permissions(*HOOK_AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA)
                    .after_remove_liquidity_returns_delta
            );
            assert!(
                permissions(ALL_HOOKS_ADDRESS).after_remove_liquidity_returns_delta
            );
            assert!(
                !permissions(*HOOK_AFTER_ADD_LIQUIDITY_RETURNS_DELTA)
                    .after_remove_liquidity_returns_delta
            );
            assert!(
                !permissions(*HOOK_BEFORE_SWAP_RETURNS_DELTA).after_remove_liquidity_returns_delta
            );
            assert!(
                !permissions(EMPTY_HOOK_ADDRESS).after_remove_liquidity_returns_delta
            );
        }
    }

    mod has_permission {
        use super::*;

        #[test]
        fn before_initialize() {
            assert!(
                has_permission(*HOOK_BEFORE_INITIALIZE, HookOptions::BeforeInitialize)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeInitialize)
            );
        }

        #[test]
        fn after_initialize() {
            assert!(
                has_permission(*HOOK_AFTER_INITIALIZE, HookOptions::AfterInitialize)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterInitialize)
            );
        }

        #[test]
        fn before_add_liquidity() {
            assert!(
                has_permission(*HOOK_BEFORE_ADD_LIQUIDITY, HookOptions::BeforeAddLiquidity)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeAddLiquidity)
            );
        }

        #[test]
        fn after_add_liquidity() {
            assert!(
                has_permission(*HOOK_AFTER_ADD_LIQUIDITY, HookOptions::AfterAddLiquidity)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterAddLiquidity)
            );
        }

        #[test]
        fn before_remove_liquidity() {
            assert!(
                has_permission(
                    *HOOK_BEFORE_REMOVE_LIQUIDITY,
                    HookOptions::BeforeRemoveLiquidity
                )
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeRemoveLiquidity)
            );
        }

        #[test]
        fn after_remove_liquidity() {
            assert!(
                has_permission(
                    *HOOK_AFTER_REMOVE_LIQUIDITY,
                    HookOptions::AfterRemoveLiquidity
                )
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterRemoveLiquidity)
            );
        }

        #[test]
        fn before_swap() {
            assert!(
                has_permission(*HOOK_BEFORE_SWAP, HookOptions::BeforeSwap)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeSwap)
            );
        }

        #[test]
        fn after_swap() {
            assert!(
                has_permission(*HOOK_AFTER_SWAP, HookOptions::AfterSwap)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterSwap)
            );
        }

        #[test]
        fn before_donate() {
            assert!(
                has_permission(*HOOK_BEFORE_DONATE, HookOptions::BeforeDonate)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeDonate)
            );
        }

        #[test]
        fn after_donate() {
            assert!(
                has_permission(*HOOK_AFTER_DONATE, HookOptions::AfterDonate)
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterDonate)
            );
        }

        #[test]
        fn before_swap_returns_delta() {
            assert!(
                has_permission(
                    *HOOK_BEFORE_SWAP_RETURNS_DELTA,
                    HookOptions::BeforeSwapReturnsDelta
                )
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::BeforeSwapReturnsDelta)
            );
        }

        #[test]
        fn after_swap_returns_delta() {
            assert!(
                has_permission(
                    *HOOK_AFTER_SWAP_RETURNS_DELTA,
                    HookOptions::AfterSwapReturnsDelta
                )
            );
            assert!(
                !has_permission(EMPTY_HOOK_ADDRESS, HookOptions::AfterSwapReturnsDelta)
            );
        }

        #[test]
        fn after_add_liquidity_returns_delta() {
            assert!(
                has_permission(
                    *HOOK_AFTER_ADD_LIQUIDITY_RETURNS_DELTA,
                    HookOptions::AfterAddLiquidityReturnsDelta
                )
            );
            assert!(
                !has_permission(
                    EMPTY_HOOK_ADDRESS,
                    HookOptions::AfterAddLiquidityReturnsDelta
                )
            );
        }

        #[test]
        fn after_remove_liquidity_returns_delta() {
            assert!(
                has_permission(
                    *HOOK_AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA,
                    HookOptions::AfterRemoveLiquidityReturnsDelta
                )
            );
            assert!(
                !has_permission(
                    EMPTY_HOOK_ADDRESS,
                    HookOptions::AfterRemoveLiquidityReturnsDelta
                )
            );
        }
    }

    mod has_initialize_permissions {
        use super::*;

        #[test]
        fn before_initialize() {
            assert!(has_initialize_permissions(*HOOK_BEFORE_INITIALIZE));
        }

        #[test]
        fn after_initialize() {
            assert!(has_initialize_permissions(*HOOK_AFTER_INITIALIZE));
        }

        #[test]
        fn non_initialize() {
            assert!(!has_initialize_permissions(*HOOK_AFTER_SWAP));
        }
    }

    mod has_liquidity_permissions {
        use super::*;

        #[test]
        fn before_add_liquidity() {
            assert!(has_liquidity_permissions(*HOOK_BEFORE_ADD_LIQUIDITY));
        }

        #[test]
        fn after_add_liquidity() {
            assert!(has_liquidity_permissions(*HOOK_AFTER_ADD_LIQUIDITY));
        }

        #[test]
        fn before_remove_liquidity() {
            assert!(
                has_liquidity_permissions(*HOOK_BEFORE_REMOVE_LIQUIDITY)
            );
        }

        #[test]
        fn after_remove_liquidity() {
            assert!(
                has_liquidity_permissions(*HOOK_AFTER_REMOVE_LIQUIDITY)
            );
        }

        #[test]
        fn non_liquidity() {
            assert!(
                !has_liquidity_permissions(*HOOK_AFTER_REMOVE_LIQUIDITY_RETURNS_DELTA)
            );
        }
    }

    mod has_swap_permissions {
        use super::*;

        #[test]
        fn before_swap() {
            assert!(has_swap_permissions(*HOOK_BEFORE_SWAP));
        }

        #[test]
        fn after_swap() {
            assert!(has_swap_permissions(*HOOK_AFTER_SWAP));
        }

        #[test]
        fn non_swap() {
            assert!(!has_swap_permissions(*HOOK_BEFORE_SWAP_RETURNS_DELTA));
        }
    }

    mod has_donate_permissions {
        use super::*;

        #[test]
        fn before_donate() {
            assert!(has_donate_permissions(*HOOK_BEFORE_DONATE));
        }

        #[test]
        fn after_donate() {
            assert!(has_donate_permissions(*HOOK_AFTER_DONATE));
        }

        #[test]
        fn non_donate() {
            assert!(!has_donate_permissions(*HOOK_AFTER_SWAP));
        }
    }
}
