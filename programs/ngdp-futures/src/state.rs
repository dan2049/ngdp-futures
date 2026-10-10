use anchor_lang::prelude::*;

/// Who is in charge. One of these exists, at the CONFIG_SEED address.
#[account]
#[derive(InitSpace)]
pub struct Config {
    /// Creates markets, funds pools, corrects reported values. (Multisig before mainnet.)
    pub admin: Pubkey,
    /// Posts the BEA estimate after each release.
    pub reporter: Pubkey,
    /// The USDC token mint used as collateral.
    pub usdc_mint: Pubkey,
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

/// Open -> Pending (value reported) -> Final (after the 24h window),
/// or Open -> Fallback (no BEA estimate 365 days after the scheduled release).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum MarketState {
    Open,
    Pending,
    Final,
    Fallback,
}

/// One quarterly futures market with its own pool market maker.
///
/// Under the hood every contract is a fully collateralised pair: an "above" side that
/// pays $1,000 + gap and a "below" side that pays $1,000 - gap (gap clamped to the band).
/// Traders never see the pair: a long position is "above" units, a short is "below" units.
/// The pool holds both sides and quotes the price.
#[account]
#[derive(InitSpace)]
pub struct Market {
    /// Quarters after Q3 2025 (1 = Q4 2025 ... 40 = Q3 2035).
    pub quarter_index: u16,
    /// Band in $bn, e.g. -1000 and +1000.
    pub floor_bn: i64,
    pub cap_bn: i64,
    /// USDC base units backing one contract's pair: (cap - floor) x $1.
    pub collateral_per_pair: u64,
    pub usdc_mint: Pubkey,
    /// Holds all the USDC. Owned by this market, so only the program's rules move it.
    pub vault: Pubkey,
    pub state: MarketState,
    pub bump: u8,
    /// Scheduled BEA advance-estimate release (unix seconds). Used for the fallback deadline.
    pub expected_release_ts: i64,
    /// Reported NGDP in tenths of $bn. 0 until reported.
    pub reported_value_tenths: i64,
    pub report_ts: i64,
    /// Final gap = reported - Level Target, tenths of $bn (can be negative).
    pub gap_tenths: i64,
    /// USDC base units paid per whole contract of the above / below side once settled.
    pub above_payout: u64,
    pub below_payout: u64,
    /// Pool balances, in millionths of a contract.
    pub pool_above: u64,
    pub pool_below: u64,
    /// Extra units owned by the pool funder from setting a starting price.
    pub lp_above: u64,
    pub lp_below: u64,
    /// Pairs in existence (millionths). Vault always holds total_sets x set price.
    pub total_sets: u64,
    pub last_trade_ts: i64,
    pub pool_withdrawn: bool,
}

impl Market {
    /// USDC base units per millionth-of-a-contract pair (2,000 for a +/-1000 band).
    pub fn set_units(&self) -> u64 {
        self.collateral_per_pair / crate::constants::UNITS_PER_CONTRACT
    }
}

/// One trader's position in one market. Never holds both sides at once:
/// buying first closes any short, selling first closes any long.
#[account]
#[derive(InitSpace)]
pub struct Position {
    pub owner: Pubkey,
    pub market: Pubkey,
    /// Long size, millionths of a contract.
    pub above: u64,
    /// Short size, millionths of a contract.
    pub below: u64,
    /// Total USDC paid in minus received, base units (for showing profit and loss).
    pub net_paid: i64,
    pub bump: u8,
}

/// Payout per whole ABOVE (long) and BELOW (short) contract for a given gap.
/// The gap is clamped to the band, so the two always add up to collateral_per_pair.
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
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, -5_000), Some((500 * USDC, 1_500 * USDC)));
    }

    #[test]
    fn fractional_gap() {
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, 1_301), Some((1_130_100_000, 869_900_000)));
    }

    #[test]
    fn clamped_at_the_edges() {
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, -14_593), Some((0, PAIR)));
        assert_eq!(payouts_for_gap(-1000, 1000, PAIR, 99_999), Some((PAIR, 0)));
    }
}
