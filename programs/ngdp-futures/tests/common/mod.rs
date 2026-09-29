//! Shared helpers for the integration tests.
#![allow(dead_code)]

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

pub use ngdp_futures::constants::*;

pub fn token_program() -> Pubkey {
    anchor_spl::token::ID
}
pub const USDC: u64 = 1_000_000; // 1 USDC in base units (6 decimals)

// ---------- helpers ----------

pub fn setup() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/ngdp_futures.so"
    ));
    svm.add_program(ngdp_futures::id(), bytes).unwrap();
    svm
}

pub fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
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

pub fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &ngdp_futures::id()).0
}

/// Write a token-program Mint account (82 bytes) directly into the chain.
pub fn put_mint(svm: &mut LiteSVM, address: Pubkey, authority: Pubkey, decimals: u8) {
    let mut d = vec![0u8; 82];
    d[0..4].copy_from_slice(&1u32.to_le_bytes()); // mint authority: Some
    d[4..36].copy_from_slice(authority.as_ref());
    d[36..44].copy_from_slice(&(1_000_000 * USDC).to_le_bytes()); // supply
    d[44] = decimals;
    d[45] = 1; // initialised
    put_account(svm, address, d);
}

/// Write a token-program token account (165 bytes) directly into the chain.
pub fn put_token_account(svm: &mut LiteSVM, address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) {
    let mut d = vec![0u8; 165];
    d[0..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = 1; // state: initialised
    put_account(svm, address, d);
}

pub fn put_account(svm: &mut LiteSVM, address: Pubkey, data: Vec<u8>) {
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
pub fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let data = svm.get_account(token_account).unwrap().data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

pub struct MarketAddrs {
    pub market: Pubkey,
    pub above_mint: Pubkey,
    pub below_mint: Pubkey,
    pub vault: Pubkey,
}

pub fn market_addrs(quarter_index: u16) -> MarketAddrs {
    let market = pda(&[MARKET_SEED, &quarter_index.to_le_bytes()]);
    MarketAddrs {
        market,
        above_mint: pda(&[ABOVE_MINT_SEED, market.as_ref()]),
        below_mint: pda(&[BELOW_MINT_SEED, market.as_ref()]),
        vault: pda(&[VAULT_SEED, market.as_ref()]),
    }
}

pub fn create_market_ix(admin: Pubkey, usdc_mint: Pubkey, quarter_index: u16, floor_bn: i64, cap_bn: i64) -> Instruction {
    create_market_ix_at(admin, usdc_mint, quarter_index, floor_bn, cap_bn, RELEASE_TS)
}

/// Same, with an explicit scheduled BEA release time.
pub fn create_market_ix_at(admin: Pubkey, usdc_mint: Pubkey, quarter_index: u16, floor_bn: i64, cap_bn: i64, expected_release_ts: i64) -> Instruction {
    let m = market_addrs(quarter_index);
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CreateMarket { quarter_index, floor_bn, cap_bn, expected_release_ts }.data(),
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

pub struct UserAccts {
    pub usdc: Pubkey,
    pub above: Pubkey,
    pub below: Pubkey,
}

pub fn pair_ix(mint: bool, user: Pubkey, usdc_mint: Pubkey, quarter_index: u16, ua: &UserAccts, amount: u64) -> Instruction {
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
pub fn setup_with_config() -> (LiteSVM, Keypair, Pubkey) {
    let (svm, admin, _reporter, usdc_mint) = setup_with_reporter();
    (svm, admin, usdc_mint)
}

/// Same, also returning the reporter (a different key from the admin).
pub fn setup_with_reporter() -> (LiteSVM, Keypair, Keypair, Pubkey) {
    let mut svm = setup();
    let admin = Keypair::new();
    let reporter = Keypair::new();
    svm.airdrop(&reporter.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let usdc_mint = Pubkey::new_unique();
    put_mint(&mut svm, usdc_mint, admin.pubkey(), 6);

    let init = Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::InitializeConfig { reporter: reporter.pubkey(), usdc_mint }.data(),
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
    (svm, admin, reporter, usdc_mint)
}

/// Scheduled release time used by default in tests: the simulated chain starts
/// near unix time 0, so "30 days from the start".
pub const RELEASE_TS: i64 = 30 * 24 * 60 * 60;

/// Move the simulated clock forward.
pub fn warp(svm: &mut LiteSVM, secs: i64) {
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += secs;
    svm.set_sysvar::<anchor_lang::prelude::Clock>(&clock);
}

pub fn now(svm: &LiteSVM) -> i64 {
    svm.get_sysvar::<anchor_lang::prelude::Clock>().unix_timestamp
}

pub fn market_state(svm: &LiteSVM, quarter_index: u16) -> ngdp_futures::state::Market {
    let data = svm.get_account(&market_addrs(quarter_index).market).unwrap().data;
    ngdp_futures::state::Market::try_deserialize(&mut data.as_slice()).unwrap()
}

/// A user holding `usdc` mock USDC plus empty ABOVE/BELOW accounts for a market.
pub fn new_user(svm: &mut LiteSVM, usdc_mint: Pubkey, quarter_index: u16, usdc: u64) -> (Keypair, UserAccts) {
    let m = market_addrs(quarter_index);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    let ua = UserAccts {
        usdc: Pubkey::new_unique(),
        above: Pubkey::new_unique(),
        below: Pubkey::new_unique(),
    };
    put_token_account(svm, ua.usdc, usdc_mint, user.pubkey(), usdc);
    put_token_account(svm, ua.above, m.above_mint, user.pubkey(), 0);
    put_token_account(svm, ua.below, m.below_mint, user.pubkey(), 0);
    (user, ua)
}

/// Send several instructions as ONE all-or-nothing transaction.
pub fn send_many(svm: &mut LiteSVM, ixs: &[Instruction], signer: &Keypair) -> bool {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let res = svm.send_transaction(tx);
    if let Err(e) = &res {
        println!("tx failed: {:?}", e.err);
    }
    res.is_ok()
}
