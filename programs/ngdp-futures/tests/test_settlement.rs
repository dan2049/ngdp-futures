//! Report -> 24h -> finalise -> claim, plus the fallback.

mod common;
use common::*;
use ngdp_futures::state::MarketState;
use solana_keypair::Keypair;
use solana_signer::Signer;

// Q3 2026 (quarter 4) Level Target = 326_529 tenths = $32,652.9bn.
const TARGET: i64 = 326_529;

#[test]
fn full_life_of_a_market() {
    let mut w = world();
    standard_market(&mut w, 0);
    let (alice, a_usdc) = trader(&mut w, 10_000 * USDC); // thinks higher
    let (bob, b_usdc) = trader(&mut w, 10_000 * USDC); // thinks lower
    assert!(do_trade(&mut w, &alice, &a_usdc, SIDE_BUY, 2 * C, true));
    assert!(do_trade(&mut w, &bob, &b_usdc, SIDE_SELL, 3 * C, true));
    assert_solvent(&w.svm, 4);

    // Only the reporter can report.
    let stranger = Keypair::new();
    w.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    assert!(!send(&mut w.svm, report_ix(&stranger.pubkey(), 4, TARGET), &stranger));

    // Release day: BEA prints $32,152.9bn => gap -$500.0bn.
    warp(&mut w.svm, 30 * DAY);
    assert!(send(&mut w.svm, report_ix(&w.reporter.pubkey(), 4, TARGET - 5_000), &w.reporter));
    assert!(market_state(&w.svm, 4).state == MarketState::Pending);
    // Trading stops once a number is in.
    assert!(!do_trade(&mut w, &alice, &a_usdc, SIDE_SELL, C, false));
    assert!(!send(&mut w.svm, finalise_ix(4), &alice));
    warp(&mut w.svm, 25 * HOUR);
    assert!(send(&mut w.svm, finalise_ix(4), &alice));
    let m = market_state(&w.svm, 4);
    assert!(m.state == MarketState::Final);
    assert_eq!(m.gap_tenths, -5_000);
    assert_eq!((m.above_payout, m.below_payout), (500 * USDC, 1_500 * USDC));

    // Longs get $500 per contract, shorts $1,500.
    let a_before = balance(&w.svm, &a_usdc);
    assert!(send(&mut w.svm, claim_ix(&alice.pubkey(), &a_usdc, &w.usdc_mint, 4), &alice));
    assert_eq!(balance(&w.svm, &a_usdc) - a_before, 1_000 * USDC);
    let b_before = balance(&w.svm, &b_usdc);
    assert!(send(&mut w.svm, claim_ix(&bob.pubkey(), &b_usdc, &w.usdc_mint, 4), &bob));
    assert_eq!(balance(&w.svm, &b_usdc) - b_before, 4_500 * USDC);
    // Nothing left to claim the second time.
    assert!(!send(&mut w.svm, claim_ix(&alice.pubkey(), &a_usdc, &w.usdc_mint, 4), &alice));
    // Alice lost, Bob won (net of what they paid).
    println!("Alice P&L ${:.2}", -(position_state(&w.svm, 4, &alice.pubkey()).net_paid as f64) / 1e6);
    println!("Bob   P&L ${:.2}", -(position_state(&w.svm, 4, &bob.pubkey()).net_paid as f64) / 1e6);
    assert!(position_state(&w.svm, 4, &alice.pubkey()).net_paid > 0);
    assert!(position_state(&w.svm, 4, &bob.pubkey()).net_paid < 0);

    // Only the admin can withdraw the pool, once; then the vault is empty (bar rounding dust).
    assert!(!send(&mut w.svm, withdraw_pool_ix(&alice.pubkey(), &a_usdc, &w.usdc_mint, 4), &alice));
    assert!(send(&mut w.svm, withdraw_pool_ix(&w.admin.pubkey(), &w.admin_usdc, &w.usdc_mint, 4), &w.admin));
    assert!(!send(&mut w.svm, withdraw_pool_ix(&w.admin.pubkey(), &w.admin_usdc, &w.usdc_mint, 4), &w.admin));
    let dust = balance(&w.svm, &vault_addr(4));
    println!("vault dust: {} base units", dust);
    assert!(dust < 10);
}

#[test]
fn fallback_pays_the_midpoint() {
    let mut w = world();
    standard_market(&mut w, -450);
    let (alice, a_usdc) = trader(&mut w, 10_000 * USDC);
    assert!(do_trade(&mut w, &alice, &a_usdc, SIDE_BUY, C, true));
    warp(&mut w.svm, 30 * DAY + 364 * DAY);
    assert!(!send(&mut w.svm, fallback_ix(4), &alice));
    warp(&mut w.svm, 2 * DAY);
    assert!(send(&mut w.svm, fallback_ix(4), &alice));
    let before = balance(&w.svm, &a_usdc);
    assert!(send(&mut w.svm, claim_ix(&alice.pubkey(), &a_usdc, &w.usdc_mint, 4), &alice));
    assert_eq!(balance(&w.svm, &a_usdc) - before, 1_000 * USDC);
    // A late report can't change a settled market.
    assert!(!send(&mut w.svm, report_ix(&w.reporter.pubkey(), 4, TARGET), &w.reporter));
}

#[test]
fn correction_restarts_the_window() {
    let mut w = world();
    standard_market(&mut w, 0);
    warp(&mut w.svm, 30 * DAY);
    assert!(send(&mut w.svm, report_ix(&w.reporter.pubkey(), 4, 330_000), &w.reporter));
    warp(&mut w.svm, 12 * HOUR);
    assert!(!send(&mut w.svm, correct_ix(&w.reporter.pubkey(), 4, TARGET), &w.reporter));
    assert!(send(&mut w.svm, correct_ix(&w.admin.pubkey(), 4, TARGET + 1_301), &w.admin));
    warp(&mut w.svm, 23 * HOUR);
    assert!(!send(&mut w.svm, finalise_ix(4), &w.reporter));
    warp(&mut w.svm, 2 * HOUR);
    assert!(send(&mut w.svm, finalise_ix(4), &w.reporter));
    let m = market_state(&w.svm, 4);
    assert_eq!((m.above_payout, m.below_payout), (1_130_100_000, 869_900_000));
}
