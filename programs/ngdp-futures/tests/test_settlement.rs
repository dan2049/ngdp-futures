//! Tests for step 3: report -> 24h challenge -> finalise -> redeem, plus the fallback.

mod common;
use anchor_lang::{
    prelude::Pubkey,
    solana_program::instruction::Instruction,
    InstructionData, ToAccountMetas,
};
use common::*;
use ngdp_futures::state::MarketState;
use solana_keypair::Keypair;
use solana_signer::Signer;

const HOUR: i64 = 60 * 60;
const DAY: i64 = 24 * HOUR;

// ---------- instruction builders ----------

fn report_ix(reporter: Pubkey, q: u16, value_tenths: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::ReportValue {
            value_tenths,
            release_url: "https://www.bea.gov/news/example".to_string(),
        }
        .data(),
        ngdp_futures::accounts::ReportValue {
            reporter,
            config: pda(&[CONFIG_SEED]),
            market: market_addrs(q).market,
        }
        .to_account_metas(None),
    )
}

fn correct_ix(admin: Pubkey, q: u16, value_tenths: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CorrectValue { value_tenths }.data(),
        ngdp_futures::accounts::CorrectValue {
            admin,
            config: pda(&[CONFIG_SEED]),
            market: market_addrs(q).market,
        }
        .to_account_metas(None),
    )
}

fn finalise_ix(q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::Finalise {}.data(),
        ngdp_futures::accounts::Finalise {
            target_table: pda(&[TARGET_TABLE_SEED]),
            market: market_addrs(q).market,
        }
        .to_account_metas(None),
    )
}

fn fallback_ix(q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::FallbackSettle {}.data(),
        ngdp_futures::accounts::FallbackSettle { market: market_addrs(q).market }.to_account_metas(None),
    )
}

fn settle_redeem_ix(user: Pubkey, usdc_mint: Pubkey, q: u16, ua: &UserAccts, above_amount: u64, below_amount: u64) -> Instruction {
    let m = market_addrs(q);
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::SettleRedeem { above_amount, below_amount }.data(),
        ngdp_futures::accounts::SettleRedeem {
            user,
            market: m.market,
            above_mint: m.above_mint,
            below_mint: m.below_mint,
            vault: m.vault,
            usdc_mint,
            user_usdc: ua.usdc,
            user_above: ua.above,
            user_below: ua.below,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

// ---------- tests ----------

/// Q4 2026 (quarter 5): target $33,053.6bn. BEA reports $32,553.6bn => gap -$500.0bn.
#[test]
fn report_correct_finalise_and_redeem() {
    let (mut svm, admin, reporter, usdc_mint) = setup_with_reporter();
    let release = now(&svm) + 30 * DAY;
    assert!(send(&mut svm, create_market_ix_at(admin.pubkey(), usdc_mint, 5, -1000, 1000, release), &admin));
    let m = market_addrs(5);

    // A trader mints 2 pairs before the release.
    let (user, ua) = new_user(&mut svm, usdc_mint, 5, 4_000 * USDC);
    assert!(send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 2), &user));
    assert_eq!(balance(&svm, &ua.usdc), 0);

    // Redeeming at settlement values is not allowed yet.
    assert!(!send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 1, 0), &user));

    // Only the reporter can report.
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    assert!(!send(&mut svm, report_ix(stranger.pubkey(), 5, 325_536), &stranger));
    assert!(!send(&mut svm, report_ix(admin.pubkey(), 5, 325_536), &admin));

    // Release day. The reporter makes a typo: $33,000.0bn.
    warp(&mut svm, 30 * DAY);
    assert!(send(&mut svm, report_ix(reporter.pubkey(), 5, 330_000), &reporter));
    assert!(market_state(&svm, 5).state == MarketState::Pending);
    // Pending: no more minting pairs, no second report, no early finalise.
    assert!(!send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 1), &user));
    assert!(!send(&mut svm, report_ix(reporter.pubkey(), 5, 325_536), &reporter));
    assert!(!send(&mut svm, finalise_ix(5), &user));

    // 12 hours later someone spots the typo; only the admin can correct it.
    warp(&mut svm, 12 * HOUR);
    assert!(!send(&mut svm, correct_ix(reporter.pubkey(), 5, 325_536), &reporter));
    assert!(send(&mut svm, correct_ix(admin.pubkey(), 5, 325_536), &admin));

    // The correction restarted the window: 23h later is still too early, 25h is fine.
    warp(&mut svm, 23 * HOUR);
    assert!(!send(&mut svm, finalise_ix(5), &user));
    warp(&mut svm, 2 * HOUR);
    assert!(send(&mut svm, finalise_ix(5), &user)); // anyone can finalise

    let market = market_state(&svm, 5);
    assert!(market.state == MarketState::Final);
    assert_eq!(market.gap_tenths, -5_000); // -$500.0bn
    assert_eq!(market.above_payout, 500 * USDC);
    assert_eq!(market.below_payout, 1_500 * USDC);

    // Settled for good: no correction, no second finalise, no fallback.
    assert!(!send(&mut svm, correct_ix(admin.pubkey(), 5, 340_000), &admin));
    assert!(!send(&mut svm, finalise_ix(5), &user));
    assert!(!send(&mut svm, fallback_ix(5), &user));

    // Redeem ABOVE and BELOW separately, then the rest together.
    assert!(send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 1, 0), &user));
    assert_eq!(balance(&svm, &ua.usdc), 500 * USDC);
    assert!(send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 0, 1), &user));
    assert_eq!(balance(&svm, &ua.usdc), 2_000 * USDC);
    // Can't redeem tokens you no longer have.
    assert!(!send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 2, 0), &user));
    assert!(send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 1, 1), &user));
    assert_eq!(balance(&svm, &ua.usdc), 4_000 * USDC);
    // Every dollar went back out: the vault is empty.
    assert_eq!(balance(&svm, &m.vault), 0);
}

/// Gaps beyond the band clamp; fractional gaps pay to the cent.
#[test]
fn clamped_and_fractional_payouts() {
    let (mut svm, admin, reporter, usdc_mint) = setup_with_reporter();
    let release = now(&svm) + DAY;
    // Q1 2027 (quarter 6): target 334_593. Report $32,000.0bn => gap -$1,459.3bn => clamped.
    assert!(send(&mut svm, create_market_ix_at(admin.pubkey(), usdc_mint, 6, -1000, 1000, release), &admin));
    // Q2 2027 (quarter 7): target 338_699. Report $34,000.0bn => gap +$130.1bn.
    assert!(send(&mut svm, create_market_ix_at(admin.pubkey(), usdc_mint, 7, -1000, 1000, release), &admin));

    assert!(send(&mut svm, report_ix(reporter.pubkey(), 6, 320_000), &reporter));
    assert!(send(&mut svm, report_ix(reporter.pubkey(), 7, 340_000), &reporter));
    warp(&mut svm, DAY);
    assert!(send(&mut svm, finalise_ix(6), &reporter));
    assert!(send(&mut svm, finalise_ix(7), &reporter));

    let q6 = market_state(&svm, 6);
    assert_eq!(q6.gap_tenths, -14_593);
    assert_eq!((q6.above_payout, q6.below_payout), (0, 2_000 * USDC));

    let q7 = market_state(&svm, 7);
    assert_eq!(q7.gap_tenths, 1_301);
    assert_eq!((q7.above_payout, q7.below_payout), (1_130_100_000, 869_900_000)); // $1,130.10 / $869.90
}

/// No BEA estimate ever arrives: after 365 days anyone can settle at the midpoint.
#[test]
fn fallback_after_365_days() {
    let (mut svm, admin, reporter, usdc_mint) = setup_with_reporter();
    let release = now(&svm) + 30 * DAY;
    assert!(send(&mut svm, create_market_ix_at(admin.pubkey(), usdc_mint, 5, -1000, 1000, release), &admin));
    let (user, ua) = new_user(&mut svm, usdc_mint, 5, 2_000 * USDC);
    assert!(send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 1), &user));

    // 364 days after the scheduled release: too early.
    warp(&mut svm, 30 * DAY + 364 * DAY);
    assert!(!send(&mut svm, fallback_ix(5), &user));
    // 366 days: allowed.
    warp(&mut svm, 2 * DAY);
    assert!(send(&mut svm, fallback_ix(5), &user));
    let market = market_state(&svm, 5);
    assert!(market.state == MarketState::Fallback);
    assert_eq!((market.above_payout, market.below_payout), (1_000 * USDC, 1_000 * USDC));

    // A late report can no longer change anything.
    assert!(!send(&mut svm, report_ix(reporter.pubkey(), 5, 330_000), &reporter));

    // Each side gets $1,000.
    assert!(send(&mut svm, settle_redeem_ix(user.pubkey(), usdc_mint, 5, &ua, 1, 1), &user));
    assert_eq!(balance(&svm, &ua.usdc), 2_000 * USDC);
}

/// If a value was reported (even very late), the fallback can't override it.
#[test]
fn no_fallback_once_reported() {
    let (mut svm, admin, reporter, usdc_mint) = setup_with_reporter();
    let release = now(&svm) + DAY;
    assert!(send(&mut svm, create_market_ix_at(admin.pubkey(), usdc_mint, 5, -1000, 1000, release), &admin));
    warp(&mut svm, 300 * DAY);
    // BEA was delayed ~300 days and published a different estimate first (option B).
    assert!(send(&mut svm, report_ix(reporter.pubkey(), 5, 331_000), &reporter));
    warp(&mut svm, 100 * DAY);
    assert!(!send(&mut svm, fallback_ix(5), &reporter));
    assert!(send(&mut svm, finalise_ix(5), &reporter));
    assert_eq!(market_state(&svm, 5).gap_tenths, 331_000 - 330_536);
}
