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

/// Where a market is in its life:
/// Open -> Pending (value reported) -> Final (after the 24h window),
/// or Open -> Fallback (no BEA estimate 365 days after the scheduled release).
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
    /// Scheduled BEA advance-estimate release (unix seconds). Only used for the fallback deadline.
    pub expected_release_ts: i64,
    /// Reported NGDP in tenths of $bn (e.g. 331_000 = $33,100.0bn). 0 until reported.
    pub reported_value_tenths: i64,
    /// When the value was reported or last corrected (unix seconds).
    pub report_ts: i64,
    /// Final gap = reported - Level Target, tenths of $bn (can be negative).
    pub gap_tenths: i64,
    /// USDC base units paid per ABOVE / BELOW token once settled.
    pub above_payout: u64,
    pub below_payout: u64,
}

/// Payout per ABOVE and BELOW token for a given gap.
/// The gap is clamped to the band, so ABOVE + BELOW always = collateral_per_pair.
/// Returns None only on arithmetic overflow.
pub fn payouts_for_gap(
    floor_bn: i64,
    cap_bn: i64,
    collateral_per_pair: u64,
    gap_tenths: i64,
) -> Option<(u64, u64)> {
    let floor = floor_bn.checked_mul(10)?;
    let cap = cap_bn.checked_mul(10)?;
    let clamped = gap_tenths.clamp(floor, cap);
    let above_tenths = clamped.checked_sub(floor)? as u64;
    // $1 per $1bn = 1_000_000 USDC units per $bn = 100_000 per tenth.
    let above = above_tenths.checked_mul(crate::constants::USDC_UNITS_PER_BN / 10)?;
    let below = collateral_per_pair.checked_sub(above)?;
    Some((above, below))
}

#[cfg(test)]
mod tests {
    use super::payouts_for_gap;
    const USDC: u64 = 1_000_000;
    const PAIR: u64 = 2_000 * USDC;

    #[test]
    fn on_target_pays_half_each() {
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, 0), Some((1_000 * USDC, 1_000 * USDC)));
    }

    #[test]
    fn below_target() {
        // Gap -$500.0bn: ABOVE $500, BELOW $1,500.
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, -5_000), Some((500 * USDC, 1_500 * USDC)));
    }

    #[test]
    fn fractional_gap() {
        // Gap +$130.1bn: ABOVE $1,130.10, BELOW $869.90.
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, 1_301), Some((1_130_100_000, 869_900_000)));
    }

    #[test]
    fn clamped_at_the_edges() {
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, -14_593), Some((0, PAIR)));
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, 99_999), Some((PAIR, 0)));
    }
}
