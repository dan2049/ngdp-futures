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
