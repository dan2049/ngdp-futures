use anchor_lang::prelude::*;

/// Seed for the single Config account (admin + reporter keys).
#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

/// Seed for the single, locked Level Target table.
#[constant]
pub const TARGET_TABLE_SEED: &[u8] = b"target_table";

/// 41 quarters: Q3 2025 (the base, n = 0) through Q3 2035 (n = 40).
pub const NUM_TARGETS: usize = 41;

/// Level Targets in TENTHS of a billion dollars (310_980 = $31,098.0bn).
///
/// Index n = number of quarters after Q3 2025.
/// Target_n = 31,098.0 x 1.05^(n/4), rounded to $0.1bn.
/// Base = BEA Q3 2025 Updated Estimate (Jan 22, 2026), frozen forever.
/// Regenerate / verify with: python3 scripts/level_targets.py
pub const LEVEL_TARGETS: [i64; NUM_TARGETS] = [
    310_980, 314_796, 318_660, 322_570, // Q3 2025 .. Q2 2026
    326_529, 330_536, 334_593, 338_699, // Q3 2026 .. Q2 2027
    342_855, 347_063, 351_322, 355_634, // Q3 2027 .. Q2 2028
    359_998, 364_416, 368_888, 373_415, // Q3 2028 .. Q2 2029
    377_998, 382_637, 387_333, 392_086, // Q3 2029 .. Q2 2030
    396_898, 401_769, 406_699, 411_691, // Q3 2030 .. Q2 2031
    416_743, 421_857, 427_034, 432_275, // Q3 2031 .. Q2 2032
    437_580, 442_950, 448_386, 453_889, // Q3 2032 .. Q2 2033
    459_459, 465_098, 470_805, 476_583, // Q3 2033 .. Q2 2034
    482_432, 488_353, 494_346, 500_412, // Q3 2034 .. Q2 2035
    506_554,                            // Q3 2035
];

/// Seeds for each quarter's market and the accounts it controls.
#[constant]
pub const MARKET_SEED: &[u8] = b"market";
#[constant]
pub const ABOVE_MINT_SEED: &[u8] = b"above";
#[constant]
pub const BELOW_MINT_SEED: &[u8] = b"below";
#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

/// USDC has 6 decimals: 1 USDC = 1_000_000 base units.
/// Multiplier is $1 per $1bn of gap, so 1 $bn of band = 1_000_000 base units.
#[constant]
pub const USDC_UNITS_PER_BN: u64 = 1_000_000;

/// After a value is reported, anyone has 24 hours to check it against BEA
/// before it can be finalised. A correction by the admin restarts the 24 hours.
#[constant]
pub const CHALLENGE_WINDOW_SECS: i64 = 24 * 60 * 60;

/// Last resort: if no BEA estimate has been reported 365 days after the
/// scheduled advance-estimate release, anyone can settle at the midpoint.
#[constant]
pub const FALLBACK_DELAY_SECS: i64 = 365 * 24 * 60 * 60;

/// Longest release URL the reporter may attach (it goes in the transaction log).
pub const MAX_URL_LEN: usize = 200;
