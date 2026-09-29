//! Tests for step 2: create a quarterly market, mint and redeem ABOVE/BELOW pairs.
//! Mock USDC is written straight into the simulated chain, so no real USDC is involved.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

use ngdp_futures::constants::*;

fn token_program() -> Pubkey {
    anchor_spl::token::ID
}
const USDC: u64 = 1_000_000; // 1 USDC in base units (6 decimals)

// ---------- helpers ----------

fn setup() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/ngdp_futures.so"
    ));
    svm.add_program(ngdp_futures::id(), bytes).unwrap();
    svm
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let res = svm.send_transaction(tx);
    if let Err(e) = &res {
        println!("tx failed: {:?}", e.err);
    }
    res.is_ok()
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &ngdp_futures::id()).0
}

/// Write a token-program Mint account (82 bytes) directly into the chain.
fn put_mint(svm: &mut LiteSVM, address: Pubkey, authority: Pubkey, decimals: u8) {
    let mut d = vec![0u8; 82];
    d[0..4].copy_from_slice(&1u32.to_le_bytes()); // mint authority: Some
    d[4..36].copy_from_slice(authority.as_ref());
    d[36..44].copy_from_slice(&(1_000_000 * USDC).to_le_bytes()); // supply
    d[44] = decimals;
    d[45] = 1; // initialised
    put_account(svm, address, d);
}

/// Write a token-program token account (165 bytes) directly into the chain.
fn put_token_account(svm: &mut LiteSVM, address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) {
    let mut d = vec![0u8; 165];
    d[0..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = 1; // state: initialised
    put_account(svm, address, d);
}

fn put_account(svm: &mut LiteSVM, address: Pubkey, data: Vec<u8>) {
    svm.set_account(
        address,
        Account {
            lamports: 10_000_000,
            data,
            owner: token_program(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

/// Token balance straight from the account bytes.
fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let data = svm.get_account(token_account).unwrap().data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

struct MarketAddrs {
    market: Pubkey,
    above_mint: Pubkey,
    below_mint: Pubkey,
    vault: Pubkey,
}

fn market_addrs(quarter_index: u16) -> MarketAddrs {
    let market = pda(&[MARKET_SEED, &quarter_index.to_le_bytes()]);
    MarketAddrs {
        market,
        above_mint: pda(&[ABOVE_MINT_SEED, market.as_ref()]),
        below_mint: pda(&[BELOW_MINT_SEED, market.as_ref()]),
        vault: pda(&[VAULT_SEED, market.as_ref()]),
    }
}

fn create_market_ix(admin: Pubkey, usdc_mint: Pubkey, quarter_index: u16, floor_bn: i64, cap_bn: i64) -> Instruction {
    let m = market_addrs(quarter_index);
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CreateMarket { quarter_index, floor_bn, cap_bn }.data(),
        ngdp_futures::accounts::CreateMarket {
            admin,
            config: pda(&[CONFIG_SEED]),
            target_table: pda(&[TARGET_TABLE_SEED]),
            market: m.market,
            above_mint: m.above_mint,
            below_mint: m.below_mint,
            vault: m.vault,
            usdc_mint,
            token_program: token_program(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct UserAccts {
    usdc: Pubkey,
    above: Pubkey,
    below: Pubkey,
}

fn pair_ix(mint: bool, user: Pubkey, usdc_mint: Pubkey, quarter_index: u16, ua: &UserAccts, amount: u64) -> Instruction {
    let m = market_addrs(quarter_index);
    if mint {
        Instruction::new_with_bytes(
            ngdp_futures::id(),
            &ngdp_futures::instruction::MintPair { amount }.data(),
            ngdp_futures::accounts::MintPair {
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
    } else {
        Instruction::new_with_bytes(
            ngdp_futures::id(),
            &ngdp_futures::instruction::RedeemPair { amount }.data(),
            ngdp_futures::accounts::RedeemPair {
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
}

/// Fresh chain with Config, TargetTable and a mock-USDC mint.
fn setup_with_config() -> (LiteSVM, Keypair, Pubkey) {
    let mut svm = setup();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let usdc_mint = Pubkey::new_unique();
    put_mint(&mut svm, usdc_mint, admin.pubkey(), 6);

    let init = Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::InitializeConfig { reporter: admin.pubkey(), usdc_mint }.data(),
        ngdp_futures::accounts::InitializeConfig {
            admin: admin.pubkey(),
            config: pda(&[CONFIG_SEED]),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut svm, init, &admin));
    let table = Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CreateTargetTable {}.data(),
        ngdp_futures::accounts::CreateTargetTable {
            admin: admin.pubkey(),
            config: pda(&[CONFIG_SEED]),
            target_table: pda(&[TARGET_TABLE_SEED]),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut svm, table, &admin));
    (svm, admin, usdc_mint)
}

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
