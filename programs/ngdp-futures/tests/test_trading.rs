//! Buying and selling against the pool.

mod common;
use common::*;
use solana_signer::Signer;

#[test]
fn buy_then_sell_back() {
    let mut w = world();
    standard_market(&mut w, 0);
    let (alice, a_usdc) = trader(&mut w, 10_000 * USDC);

    // Buy 1 contract at a price of about 0 ($bn): costs about $1,000 (+ about 1% price impact).
    assert!(do_trade(&mut w, &alice, &a_usdc, SIDE_BUY, C, true));
    let paid = 10_000 * USDC - balance(&w.svm, &a_usdc);
    println!("1 contract cost ${:.2}", paid as f64 / 1e6);
    assert!(paid > 1_000 * USDC && paid < 1_025 * USDC);
    let p = position_state(&w.svm, 4, &alice.pubkey());
    assert_eq!((p.above, p.below), (C, 0));
    assert_eq!(p.net_paid, paid as i64);
    // Buying pushed the price up.
    let gap_after = pool_gap(&market_state(&w.svm, 4));
    println!("price after buy: {:.1}bn", gap_after);
    assert!(gap_after > 0.0 && gap_after < 30.0);
    assert_solvent(&w.svm, 4);

    // Sell it back: get slightly less than paid (the round trip costs the price impact).
    assert!(do_trade(&mut w, &alice, &a_usdc, SIDE_SELL, C, false));
    let after = balance(&w.svm, &a_usdc);
    let round_trip_loss = 10_000 * USDC - after;
    println!("round trip cost ${:.2}", round_trip_loss as f64 / 1e6);
    assert!(round_trip_loss < 50 * USDC);
    let p = position_state(&w.svm, 4, &alice.pubkey());
    assert_eq!((p.above, p.below), (0, 0));
    assert!((pool_gap(&market_state(&w.svm, 4))).abs() < 0.01); // back to where it started
    assert_solvent(&w.svm, 4);
}

#[test]
fn selling_from_flat_goes_short_and_buying_flips_long() {
    let mut w = world();
    standard_market(&mut w, -450);
    let (bob, b_usdc) = trader(&mut w, 10_000 * USDC);

    // Sell 2 at about -450: a short pays (cap - price) = about $1,450 each.
    assert!(do_trade(&mut w, &bob, &b_usdc, SIDE_SELL, 2 * C, true));
    let paid = 10_000 * USDC - balance(&w.svm, &b_usdc);
    println!("short 2 at -450 cost ${:.2}", paid as f64 / 1e6);
    assert!(paid > 2_900 * USDC && paid < 2_990 * USDC);
    let p = position_state(&w.svm, 4, &bob.pubkey());
    assert_eq!((p.above, p.below), (0, 2 * C));
    assert!(pool_gap(&market_state(&w.svm, 4)) < -450.0); // selling pushed the price down
    assert_solvent(&w.svm, 4);

    // Buy 3: closes the 2 short and opens 1 long, in one trade.
    assert!(do_trade(&mut w, &bob, &b_usdc, SIDE_BUY, 3 * C, false));
    let p = position_state(&w.svm, 4, &bob.pubkey());
    assert_eq!((p.above, p.below), (C, 0));
    assert_solvent(&w.svm, 4);

    // Fractions work: sell half a contract.
    assert!(do_trade(&mut w, &bob, &b_usdc, SIDE_SELL, C / 2, false));
    assert_eq!(position_state(&w.svm, 4, &bob.pubkey()).above, C / 2);
    assert_solvent(&w.svm, 4);
}

#[test]
fn price_limit_and_funds_are_enforced() {
    let mut w = world();
    standard_market(&mut w, 0);
    let (carol, c_usdc) = trader(&mut w, 1_500 * USDC);
    assert!(send(&mut w.svm, open_position_ix(&carol.pubkey(), 4), &carol));

    // Limit of $1,000 for 1 contract: the real cost is a bit higher, so nothing happens.
    let ix = trade_ix(&carol.pubkey(), &c_usdc, &w.usdc_mint, 4, SIDE_BUY, C, 1_000 * USDC as i64);
    assert!(!send(&mut w.svm, ix, &carol));
    assert_eq!(balance(&w.svm, &c_usdc), 1_500 * USDC);
    // With a $1,100 limit it goes through.
    let ix = trade_ix(&carol.pubkey(), &c_usdc, &w.usdc_mint, 4, SIDE_BUY, C, 1_100 * USDC as i64);
    assert!(send(&mut w.svm, ix, &carol));
    // She can't afford another.
    assert!(!do_trade(&mut w, &carol, &c_usdc, SIDE_BUY, C, false));
    // Selling with a limit: insist on receiving at least $1,100 back (too much) -> refused.
    let ix = trade_ix(&carol.pubkey(), &c_usdc, &w.usdc_mint, 4, SIDE_SELL, C, -(1_100 * USDC as i64));
    assert!(!send(&mut w.svm, ix, &carol));
    // Zero quantity and a made-up side are refused.
    assert!(!send(&mut w.svm, trade_ix(&carol.pubkey(), &c_usdc, &w.usdc_mint, 4, SIDE_BUY, 0, i64::MAX), &carol));
    assert!(!send(&mut w.svm, trade_ix(&carol.pubkey(), &c_usdc, &w.usdc_mint, 4, 7, C, i64::MAX), &carol));
    assert_solvent(&w.svm, 4);
}

#[test]
fn you_cannot_trade_someone_elses_position() {
    let mut w = world();
    standard_market(&mut w, 0);
    let (victim, v_usdc) = trader(&mut w, 5_000 * USDC);
    let (thief, t_usdc) = trader(&mut w, 5_000 * USDC);
    assert!(do_trade(&mut w, &victim, &v_usdc, SIDE_BUY, C, true));
    // Thief points at the victim's position: the address check fails.
    let mut ix = trade_ix(&thief.pubkey(), &t_usdc, &w.usdc_mint, 4, SIDE_SELL, C, i64::MAX);
    ix.accounts[2].pubkey = position_addr(4, &victim.pubkey());
    assert!(!send(&mut w.svm, ix, &thief));
    // Thief can't spend the victim's USDC either.
    assert!(send(&mut w.svm, open_position_ix(&thief.pubkey(), 4), &thief));
    let ix = trade_ix(&thief.pubkey(), &v_usdc, &w.usdc_mint, 4, SIDE_BUY, C, i64::MAX);
    assert!(!send(&mut w.svm, ix, &thief));
    assert_eq!(position_state(&w.svm, 4, &victim.pubkey()).above, C);
}
