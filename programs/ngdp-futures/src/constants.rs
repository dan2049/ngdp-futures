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
