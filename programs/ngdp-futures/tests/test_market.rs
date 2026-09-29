//! Tests for step 2: create a quarterly market, mint and redeem ABOVE/BELOW pairs.
//! Mock USDC is written straight into the simulated chain, so no real USDC is involved.

mod common;
use common::*;
use solana_keypair::Keypair;
use solana_signer::Signer;
use anchor_lang::{prelude::Pubkey, AccountDeserialize};

// ---------- tests ----------

#[test]
fn create_market_rules() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();

    // Admin opens Q4 2026 (quarter 5) with a +/- $1,000bn band.
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 5, -1000, 1000), &admin));
    let data = svm.get_account(&market_addrs(5).market).unwrap().data;
    let market = ngdp_futures::state::Market::try_deserialize(&mut data.as_slice()).unwrap();
    assert_eq!(market.quarter_index, 5);
    assert_eq!(market.floor_bn, -1000);
    assert_eq!(market.cap_bn, 1000);
    assert_eq!(market.collateral_per_pair, 2_000 * USDC); // $2,000 per pair
    assert!(market.state == ngdp_futures::state::MarketState::Open);

    // Same quarter twice: rejected.
    assert!(!send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 5, -1000, 1000), &admin));
    // Stranger: rejected.
    assert!(!send(&mut svm, create_market_ix(stranger.pubkey(), usdc_mint, 6, -1000, 1000), &stranger));
    // Quarter 0 (the base) and quarter 41 (past the table): rejected.
    assert!(!send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 0, -1000, 1000), &admin));
    assert!(!send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 41, -1000, 1000), &admin));
    // Floor not below cap: rejected.
    assert!(!send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 6, 500, 500), &admin));
    // Wrong USDC mint: rejected.
    let fake_usdc = Pubkey::new_unique();
    put_mint(&mut svm, fake_usdc, admin.pubkey(), 6);
    assert!(!send(&mut svm, create_market_ix(admin.pubkey(), fake_usdc, 6, -1000, 1000), &admin));
}

#[test]
fn mint_and_redeem_pairs() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 5, -1000, 1000), &admin));
    let m = market_addrs(5);

    // A user with $5,000 of mock USDC and empty ABOVE / BELOW accounts.
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    let ua = UserAccts {
        usdc: Pubkey::new_unique(),
        above: Pubkey::new_unique(),
        below: Pubkey::new_unique(),
    };
    put_token_account(&mut svm, ua.usdc, usdc_mint, user.pubkey(), 5_000 * USDC);
    put_token_account(&mut svm, ua.above, m.above_mint, user.pubkey(), 0);
    put_token_account(&mut svm, ua.below, m.below_mint, user.pubkey(), 0);

    // Mint 2 pairs: $4,000 in, 2 ABOVE + 2 BELOW out.
    assert!(send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 2), &user));
    assert_eq!(balance(&svm, &ua.usdc), 1_000 * USDC);
    assert_eq!(balance(&svm, &m.vault), 4_000 * USDC);
    assert_eq!(balance(&svm, &ua.above), 2);
    assert_eq!(balance(&svm, &ua.below), 2);

    // Only $1,000 left, so a 3rd pair ($2,000) must fail and change nothing.
    assert!(!send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 1), &user));
    assert_eq!(balance(&svm, &ua.usdc), 1_000 * USDC);
    // Zero pairs: rejected.
    assert!(!send(&mut svm, pair_ix(true, user.pubkey(), usdc_mint, 5, &ua, 0), &user));

    // Redeem 1 pair: $2,000 back.
    assert!(send(&mut svm, pair_ix(false, user.pubkey(), usdc_mint, 5, &ua, 1), &user));
    assert_eq!(balance(&svm, &ua.usdc), 3_000 * USDC);
    assert_eq!(balance(&svm, &m.vault), 2_000 * USDC);
    assert_eq!(balance(&svm, &ua.above), 1);
    assert_eq!(balance(&svm, &ua.below), 1);

    // Can't redeem more pairs than you hold.
    assert!(!send(&mut svm, pair_ix(false, user.pubkey(), usdc_mint, 5, &ua, 2), &user));

    // Vault always equals total_pairs x $2,000.
    let data = svm.get_account(&m.market).unwrap().data;
    let market = ngdp_futures::state::Market::try_deserialize(&mut data.as_slice()).unwrap();
    assert_eq!(market.total_pairs, 1);
    assert_eq!(balance(&svm, &m.vault), market.total_pairs * market.collateral_per_pair);
}

#[test]
fn someone_else_cannot_spend_your_usdc() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, 5, -1000, 1000), &admin));
    let m = market_addrs(5);

    let victim = Keypair::new();
    let thief = Keypair::new();
    svm.airdrop(&thief.pubkey(), 1_000_000_000).unwrap();
    // Victim's USDC; thief's own ABOVE/BELOW accounts.
    let ua = UserAccts {
        usdc: Pubkey::new_unique(),
        above: Pubkey::new_unique(),
        below: Pubkey::new_unique(),
    };
    put_token_account(&mut svm, ua.usdc, usdc_mint, victim.pubkey(), 5_000 * USDC);
    put_token_account(&mut svm, ua.above, m.above_mint, thief.pubkey(), 0);
    put_token_account(&mut svm, ua.below, m.below_mint, thief.pubkey(), 0);

    assert!(!send(&mut svm, pair_ix(true, thief.pubkey(), usdc_mint, 5, &ua, 1), &thief));
    assert_eq!(balance(&svm, &ua.usdc), 5_000 * USDC);
}
