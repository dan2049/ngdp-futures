use anchor_lang::prelude::*;

/// Who is in charge. One of these exists, at the CONFIG_SEED address.
#[account]
#[derive(InitSpace)]
pub struct Config {
    /// Can create markets and correct reported values. (Multisig before mainnet.)
    pub admin: Pubkey,
    /// Posts the BEA advance estimate after each release.
    pub reporter: Pubkey,
    /// The USDC token mint used as collateral.
    pub usdc_mint: Pubkey,
    /// Stored so later instructions can re-derive this address cheaply.
    pub bump: u8,
}

/// The 41 Level Targets. Written once, and no instruction can ever change them.
#[account]
pub struct TargetTable {
    /// Tenths of $bn, index = quarters after Q3 2025 (see constants.rs).
    pub targets: [i64; 41],
    pub bump: u8,
}

impl TargetTable {
    /// Bytes needed on-chain: 8 (Anchor's account label) + 41 x 8 + 1.
    pub const SPACE: usize = 8 + 8 * 41 + 1;

    /// Level Target for quarter n, or None if n is past Q3 2035.
    pub fn target_for(&self, quarter_index: u16) -> Option<i64> {
        self.targets.get(quarter_index as usize).copied()
    }
}

/// Where a market is in its life. Step 2 only uses Open;
/// Pending / Final / Fallback arrive with settlement.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum MarketState {
    Open,
    Pending,
    Final,
    Fallback,
}

/// One quarterly contract, e.g. quarter_index 5 = Q4 2026.
#[account]
#[derive(InitSpace)]
pub struct Market {
    /// Quarters after Q3 2025 (1 = Q4 2025 ... 40 = Q3 2035).
    pub quarter_index: u16,
    /// Band in $bn, e.g. -1000 and +1000.
    pub floor_bn: i64,
    pub cap_bn: i64,
    /// USDC base units locked per ABOVE+BELOW pair: (cap - floor) x $1.
    pub collateral_per_pair: u64,
    pub usdc_mint: Pubkey,
    pub above_mint: Pubkey,
    pub below_mint: Pubkey,
    /// Token account holding every pair's USDC. Owned by this market.
    pub vault: Pubkey,
    pub state: MarketState,
    /// Pairs currently in existence.
    pub total_pairs: u64,
    pub bump: u8,
}
