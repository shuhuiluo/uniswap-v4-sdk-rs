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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

pub fn permissions(address: Address) -> HookPermissions {
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

pub fn has_permission(address: Address, hook_option: HookOptions) -> bool {
    let mask = (address.0 .0[18] as u64) << 8 | (address.0 .0[19] as u64);
    let hook_flag_index = hook_option as u64;
    mask & (1 << hook_flag_index) != 0
}

pub fn has_initialize_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeInitialize)
        || has_permission(address, HookOptions::AfterInitialize)
}

pub fn has_liquidity_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeAddLiquidity)
        || has_permission(address, HookOptions::AfterAddLiquidity)
        || has_permission(address, HookOptions::BeforeRemoveLiquidity)
        || has_permission(address, HookOptions::AfterRemoveLiquidity)
}

pub fn has_swap_permissions(address: Address) -> bool {
    // this implicitly encapsulates swap delta permissions
    has_permission(address, HookOptions::BeforeSwap)
        || has_permission(address, HookOptions::AfterSwap)
}

pub fn has_donate_permissions(address: Address) -> bool {
    has_permission(address, HookOptions::BeforeDonate)
        || has_permission(address, HookOptions::AfterDonate)
}
