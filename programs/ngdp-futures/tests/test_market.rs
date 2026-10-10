//! Creating a market and funding its pool.

mod common;
use anchor_lang::prelude::Pubkey;
use common::*;
use solana_signer::Signer;

#[test]
fn create_market_funds_an_even_pool() {
    let mut w = world();
    standard_market(&mut w, 0);
    let m = market_state(&w.svm, 4);
    assert_eq!(m.collateral_per_pair, 2_000 * USDC);
    assert_eq!(m.set_units(), 2_000);
    // $100,000 / $2,000 = 50 contracts each side.
    assert_eq!(m.pool_above, 50 * C);
    assert_eq!(m.pool_below, 50 * C);
    assert_eq!((m.lp_above, m.lp_below), (0, 0));
    assert!((pool_gap(&m) - 0.0).abs() < 1e-9);
    assert_eq!(balance(&w.svm, &w.admin_usdc), 900_000 * USDC);
    assert_solvent(&w.svm, 4);
}

#[test]
fn starting_price_can_be_negative() {
    let mut w = world();
    standard_market(&mut w, -450);
    let m = market_state(&w.svm, 4);
    let gap = pool_gap(&m);
    assert!((gap + 450.0).abs() < 0.01, "pool opens at {}", gap);
    // The leftover side goes to the funder, and nothing is lost.
    assert_eq!(m.pool_above + m.lp_above, m.total_sets);
    assert_eq!(m.pool_below + m.lp_below, m.total_sets);
    assert_solvent(&w.svm, 4);
}

#[test]
fn create_market_rules() {
    let mut w = world();
    let release = now(&w.svm) + 30 * DAY;
    let bad = [
        create_market_ix(&w, 0, -1000, 1000, release, POOL, 0),      // the base quarter
        create_market_ix(&w, 41, -1000, 1000, release, POOL, 0),     // past the table
        create_market_ix(&w, 4, 500, 500, release, POOL, 500),       // empty band
        create_market_ix(&w, 4, -1000, 1000, release, POOL, -1000),  // start on the edge
        create_market_ix(&w, 4, -1000, 1000, release, POOL, 1200),   // start outside
        create_market_ix(&w, 4, -1000, 1000, release, 1_999 * USDC, 0), // less than one contract
        create_market_ix(&w, 4, -1000, 1000, release, POOL + 1, 0),  // not whole micro-sets
    ];
    for ix in bad {
        assert!(!send(&mut w.svm, ix, &w.admin));
    }
    // A stranger can't create one, even with their own USDC.
    let (stranger, s_usdc) = trader(&mut w, 200_000 * USDC);
    let ix = create_market_ix_as(&stranger.pubkey(), &s_usdc, &w.usdc_mint, 4, -1000, 1000, release, POOL, 0);
    assert!(!send(&mut w.svm, ix, &stranger));
    // A fake USDC mint is rejected.
    let fake = Pubkey::new_unique();
    put_mint(&mut w.svm, fake, w.admin.pubkey(), 6);
    let fake_acct = Pubkey::new_unique();
    put_token_account(&mut w.svm, fake_acct, fake, w.admin.pubkey(), POOL);
    let ix = create_market_ix_as(&w.admin.pubkey(), &fake_acct, &fake, 4, -1000, 1000, release, POOL, 0);
    assert!(!send(&mut w.svm, ix, &w.admin));
    // The real one works once, and only once.
    let good = create_market_ix(&w, 4, -1000, 1000, release, POOL, 0);
    assert!(send(&mut w.svm, good.clone(), &w.admin));
    assert!(!send(&mut w.svm, good, &w.admin));
}
