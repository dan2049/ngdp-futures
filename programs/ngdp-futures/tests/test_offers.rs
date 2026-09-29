//! Tests for step 4: the ABOVE/USDC offer board and one-transaction "go short".

mod common;
use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use common::*;
use ngdp_futures::state::OfferSide;
use solana_signer::Signer;

const Q: u16 = 5; // Q4 2026

fn offer_addr(maker: Pubkey, id: u64) -> Pubkey {
    pda(&[OFFER_SEED, market_addrs(Q).market.as_ref(), maker.as_ref(), &id.to_le_bytes()])
}
fn escrow_addr(offer: Pubkey) -> Pubkey {
    pda(&[ESCROW_SEED, offer.as_ref()])
}

/// Maker posts an offer. Selling: escrow ABOVE, receive USDC. Buying: the reverse.
fn post_ix(maker: Pubkey, ua: &UserAccts, usdc_mint: Pubkey, id: u64, side: OfferSide, price: u64, quantity: u64) -> Instruction {
    let m = market_addrs(Q);
    let offer = offer_addr(maker, id);
    let (escrow_mint, source, receive) = match side {
        OfferSide::SellAbove => (m.above_mint, ua.above, ua.usdc),
        OfferSide::BuyAbove => (usdc_mint, ua.usdc, ua.above),
    };
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::PostOffer { offer_id: id, side, price, quantity }.data(),
        ngdp_futures::accounts::PostOffer {
            maker,
            market: m.market,
            offer,
            escrow: escrow_addr(offer),
            escrow_mint,
            maker_source: source,
            maker_receive: receive,
            token_program: token_program(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn fill_ix(taker: Pubkey, tua: &UserAccts, usdc_mint: Pubkey, maker: Pubkey, maker_receive: Pubkey, id: u64, quantity: u64) -> Instruction {
    let m = market_addrs(Q);
    let offer = offer_addr(maker, id);
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::FillOffer { quantity }.data(),
        ngdp_futures::accounts::FillOffer {
            taker,
            market: m.market,
            offer,
            escrow: escrow_addr(offer),
            maker_receive,
            above_mint: m.above_mint,
            usdc_mint,
            taker_usdc: tua.usdc,
            taker_above: tua.above,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

fn cancel_ix(maker: Pubkey, id: u64, escrow_mint: Pubkey, refund: Pubkey) -> Instruction {
    let offer = offer_addr(maker, id);
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CancelOffer {}.data(),
        ngdp_futures::accounts::CancelOffer {
            maker,
            offer,
            escrow: escrow_addr(offer),
            escrow_mint,
            maker_refund: refund,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

fn is_closed(svm: &litesvm::LiteSVM, addr: &Pubkey) -> bool {
    svm.get_account(addr).map_or(true, |a| a.lamports == 0)
}

#[test]
fn sell_offer_partial_fill_and_cancel() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, Q, -1000, 1000), &admin));
    let m = market_addrs(Q);

    // Maker mints 3 pairs, then offers the 3 ABOVE at $600 each.
    let (maker, mua) = new_user(&mut svm, usdc_mint, Q, 6_000 * USDC);
    assert!(send(&mut svm, pair_ix(true, maker.pubkey(), usdc_mint, Q, &mua, 3), &maker));
    assert!(send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 600 * USDC, 3), &maker));
    assert_eq!(balance(&svm, &mua.above), 0);
    assert_eq!(balance(&svm, &escrow_addr(offer_addr(maker.pubkey(), 1))), 3);

    // A buyer takes 1 for $600.
    let (buyer, bua) = new_user(&mut svm, usdc_mint, Q, 1_000 * USDC);
    assert!(send(&mut svm, fill_ix(buyer.pubkey(), &bua, usdc_mint, maker.pubkey(), mua.usdc, 1, 1), &buyer));
    assert_eq!(balance(&svm, &bua.above), 1);
    assert_eq!(balance(&svm, &bua.usdc), 400 * USDC);
    assert_eq!(balance(&svm, &mua.usdc), 600 * USDC);

    // Headline: last trade $600 => implied gap = -1000 + 600 = -$400bn.
    let market = market_state(&svm, Q);
    assert_eq!(market.last_price, 600 * USDC);
    assert_eq!(market.floor_bn + (market.last_price / USDC) as i64, -400);

    // Buyer can't afford 2 more ($1,200 > $400); can't take more than is left either.
    assert!(!send(&mut svm, fill_ix(buyer.pubkey(), &bua, usdc_mint, maker.pubkey(), mua.usdc, 1, 2), &buyer));
    assert!(!send(&mut svm, fill_ix(buyer.pubkey(), &bua, usdc_mint, maker.pubkey(), mua.usdc, 1, 3), &buyer));
    // The maker can't fill their own offer.
    assert!(!send(&mut svm, fill_ix(maker.pubkey(), &mua, usdc_mint, maker.pubkey(), mua.usdc, 1, 1), &maker));
    // Proceeds can't be redirected to someone else's account.
    assert!(!send(&mut svm, fill_ix(buyer.pubkey(), &bua, usdc_mint, maker.pubkey(), bua.usdc, 1, 1), &buyer));

    // Only the maker can cancel; they get the 2 unsold ABOVE back and the accounts close.
    assert!(!send(&mut svm, cancel_ix(buyer.pubkey(), 1, m.above_mint, bua.above), &buyer));
    assert!(send(&mut svm, cancel_ix(maker.pubkey(), 1, m.above_mint, mua.above), &maker));
    assert_eq!(balance(&svm, &mua.above), 2);
    assert!(is_closed(&svm, &offer_addr(maker.pubkey(), 1)));
    assert!(is_closed(&svm, &escrow_addr(offer_addr(maker.pubkey(), 1))));
    // Nothing left to fill.
    assert!(!send(&mut svm, fill_ix(buyer.pubkey(), &bua, usdc_mint, maker.pubkey(), mua.usdc, 1, 1), &buyer));
}

#[test]
fn buy_offer_and_one_click_go_short() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, Q, -1000, 1000), &admin));

    // A bull bids $450 each for 2 ABOVE: $900 goes into escrow.
    let (bull, lua) = new_user(&mut svm, usdc_mint, Q, 1_000 * USDC);
    assert!(send(&mut svm, post_ix(bull.pubkey(), &lua, usdc_mint, 7, OfferSide::BuyAbove, 450 * USDC, 2), &bull));
    assert_eq!(balance(&svm, &lua.usdc), 100 * USDC);

    // A bear goes short in ONE transaction: mint a pair, sell the ABOVE into the bid.
    let (bear, rua) = new_user(&mut svm, usdc_mint, Q, 2_000 * USDC);
    let go_short = [
        pair_ix(true, bear.pubkey(), usdc_mint, Q, &rua, 1),
        fill_ix(bear.pubkey(), &rua, usdc_mint, bull.pubkey(), lua.above, 7, 1),
    ];
    assert!(send_many(&mut svm, &go_short, &bear));
    // Bear: paid $2,000, got $450 back, holds 1 BELOW => risked $1,550 on "below target".
    assert_eq!(balance(&svm, &rua.usdc), 450 * USDC);
    assert_eq!(balance(&svm, &rua.below), 1);
    assert_eq!(balance(&svm, &rua.above), 0);
    assert_eq!(balance(&svm, &lua.above), 1);
    // Implied gap = -1000 + 450 = -$550bn.
    assert_eq!(market_state(&svm, Q).last_price, 450 * USDC);

    // All-or-nothing: a second bear asks for 5 when only 1 is left, so the mint is undone too.
    let (bear2, r2) = new_user(&mut svm, usdc_mint, Q, 2_000 * USDC);
    let too_big = [
        pair_ix(true, bear2.pubkey(), usdc_mint, Q, &r2, 1),
        fill_ix(bear2.pubkey(), &r2, usdc_mint, bull.pubkey(), lua.above, 7, 5),
    ];
    assert!(!send_many(&mut svm, &too_big, &bear2));
    assert_eq!(balance(&svm, &r2.usdc), 2_000 * USDC);
    assert_eq!(balance(&svm, &r2.below), 0);

    // Bull cancels: the unused $450 comes back.
    assert!(send(&mut svm, cancel_ix(bull.pubkey(), 7, usdc_mint, lua.usdc), &bull));
    assert_eq!(balance(&svm, &lua.usdc), 550 * USDC);
}

#[test]
fn offer_price_and_account_checks() {
    let (mut svm, admin, usdc_mint) = setup_with_config();
    assert!(send(&mut svm, create_market_ix(admin.pubkey(), usdc_mint, Q, -1000, 1000), &admin));
    let (maker, mua) = new_user(&mut svm, usdc_mint, Q, 10_000 * USDC);
    assert!(send(&mut svm, pair_ix(true, maker.pubkey(), usdc_mint, Q, &mua, 2), &maker));

    // Price 0 and price above $2,000 (a whole pair) are rejected.
    assert!(!send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 0, 1), &maker));
    assert!(!send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 2_001 * USDC, 1), &maker));
    // Can't sell more ABOVE than you hold.
    assert!(!send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 500 * USDC, 3), &maker));
    // Trying to sell BELOW on the ABOVE board is rejected.
    let m = market_addrs(Q);
    let mut sneaky = post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 500 * USDC, 1);
    sneaky.accounts[4].pubkey = m.below_mint; // escrow_mint
    sneaky.accounts[5].pubkey = mua.below; // maker_source
    assert!(!send(&mut svm, sneaky, &maker));
    // A valid offer works; reusing its id doesn't.
    assert!(send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 2_000 * USDC, 1), &maker));
    assert!(!send(&mut svm, post_ix(maker.pubkey(), &mua, usdc_mint, 1, OfferSide::SellAbove, 500 * USDC, 1), &maker));
}
