//! Shared helpers for the integration tests.
#![allow(dead_code)]

use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
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

pub const USDC: u64 = 1_000_000; // 1 USDC in base units
pub const C: u64 = 1_000_000; // 1 contract in position units
pub const HOUR: i64 = 60 * 60;
pub const DAY: i64 = 24 * HOUR;
pub const POOL: u64 = 100_000 * USDC; // default pool liquidity

pub fn token_program() -> Pubkey {
    anchor_spl::token::ID
}

pub fn setup() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/ngdp_futures.so"));
    svm.add_program(ngdp_futures::id(), bytes).unwrap();
    svm
}

pub fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    send_many(svm, &[ix], signer)
}

/// Several instructions as ONE all-or-nothing transaction.
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

pub fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &ngdp_futures::id()).0
}
pub fn market_addr(q: u16) -> Pubkey {
    pda(&[MARKET_SEED, &q.to_le_bytes()])
}
pub fn vault_addr(q: u16) -> Pubkey {
    pda(&[VAULT_SEED, market_addr(q).as_ref()])
}
pub fn position_addr(q: u16, user: &Pubkey) -> Pubkey {
    pda(&[POSITION_SEED, market_addr(q).as_ref(), user.as_ref()])
}

// ---------- fake token accounts written straight into the simulated chain ----------

pub fn put_mint(svm: &mut LiteSVM, address: Pubkey, authority: Pubkey, decimals: u8) {
    let mut d = vec![0u8; 82];
    d[0..4].copy_from_slice(&1u32.to_le_bytes());
    d[4..36].copy_from_slice(authority.as_ref());
    d[36..44].copy_from_slice(&(100_000_000 * USDC).to_le_bytes());
    d[44] = decimals;
    d[45] = 1;
    put_account(svm, address, d);
}

pub fn put_token_account(svm: &mut LiteSVM, address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) {
    let mut d = vec![0u8; 165];
    d[0..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = 1;
    put_account(svm, address, d);
}

fn put_account(svm: &mut LiteSVM, address: Pubkey, data: Vec<u8>) {
    svm.set_account(
        address,
        Account { lamports: 10_000_000, data, owner: token_program(), executable: false, rent_epoch: 0 },
    )
    .unwrap();
}

pub fn balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let data = svm.get_account(token_account).unwrap().data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

// ---------- world setup ----------

pub struct World {
    pub svm: LiteSVM,
    pub admin: Keypair,
    pub admin_usdc: Pubkey,
    pub reporter: Keypair,
    pub usdc_mint: Pubkey,
}

/// Config + Level Targets + mock USDC; the admin holds $1,000,000 of it.
pub fn world() -> World {
    let mut svm = setup();
    let admin = Keypair::new();
    let reporter = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&reporter.pubkey(), 1_000_000_000).unwrap();
    let usdc_mint = Pubkey::new_unique();
    put_mint(&mut svm, usdc_mint, admin.pubkey(), 6);
    let admin_usdc = Pubkey::new_unique();
    put_token_account(&mut svm, admin_usdc, usdc_mint, admin.pubkey(), 1_000_000 * USDC);

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
    World { svm, admin, admin_usdc, reporter, usdc_mint }
}

/// A trader with `usdc` mock USDC and SOL for fees.
pub fn trader(w: &mut World, usdc: u64) -> (Keypair, Pubkey) {
    let user = Keypair::new();
    w.svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    let acct = Pubkey::new_unique();
    put_token_account(&mut w.svm, acct, w.usdc_mint, user.pubkey(), usdc);
    (user, acct)
}

pub fn now(svm: &LiteSVM) -> i64 {
    svm.get_sysvar::<Clock>().unix_timestamp
}
pub fn warp(svm: &mut LiteSVM, secs: i64) {
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += secs;
    svm.set_sysvar::<Clock>(&clock);
}

pub fn market_state(svm: &LiteSVM, q: u16) -> ngdp_futures::state::Market {
    let data = svm.get_account(&market_addr(q)).unwrap().data;
    ngdp_futures::state::Market::try_deserialize(&mut data.as_slice()).unwrap()
}
pub fn position_state(svm: &LiteSVM, q: u16, user: &Pubkey) -> ngdp_futures::state::Position {
    let data = svm.get_account(&position_addr(q, user)).unwrap().data;
    ngdp_futures::state::Position::try_deserialize(&mut data.as_slice()).unwrap()
}

/// Implied gap ($bn) from the pool: floor + width * below / (above + below).
pub fn pool_gap(m: &ngdp_futures::state::Market) -> f64 {
    m.floor_bn as f64 + (m.cap_bn - m.floor_bn) as f64 * m.pool_below as f64 / (m.pool_above + m.pool_below) as f64
}

/// The vault must always hold exactly total_sets x set price before settlement.
pub fn assert_solvent(svm: &LiteSVM, q: u16) {
    let m = market_state(svm, q);
    assert_eq!(balance(svm, &vault_addr(q)), m.total_sets * m.set_units(), "vault out of balance");
}

// ---------- instruction builders ----------

pub fn create_market_ix(w: &World, q: u16, floor_bn: i64, cap_bn: i64, release: i64, liquidity: u64, start_gap: i64) -> Instruction {
    create_market_ix_as(&w.admin.pubkey(), &w.admin_usdc, &w.usdc_mint, q, floor_bn, cap_bn, release, liquidity, start_gap)
}

pub fn create_market_ix_as(admin: &Pubkey, admin_usdc: &Pubkey, usdc_mint: &Pubkey, q: u16, floor_bn: i64, cap_bn: i64,
    release: i64, liquidity: u64, starting_gap_bn: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CreateMarket {
            quarter_index: q,
            floor_bn,
            cap_bn,
            expected_release_ts: release,
            liquidity,
            starting_gap_bn,
        }
        .data(),
        ngdp_futures::accounts::CreateMarket {
            admin: *admin,
            config: pda(&[CONFIG_SEED]),
            target_table: pda(&[TARGET_TABLE_SEED]),
            market: market_addr(q),
            vault: vault_addr(q),
            usdc_mint: *usdc_mint,
            admin_usdc: *admin_usdc,
            token_program: token_program(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Standard test market: Q3 2026 (quarter 4), +/-1000 band, $100k pool, opening at `start_gap`.
pub fn standard_market(w: &mut World, start_gap: i64) {
    let release = now(&w.svm) + 30 * DAY;
    let ix = create_market_ix(w, 4, -1000, 1000, release, POOL, start_gap);
    assert!(send(&mut w.svm, ix, &w.admin));
}

pub fn open_position_ix(user: &Pubkey, q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::OpenPosition {}.data(),
        ngdp_futures::accounts::OpenPosition {
            user: *user,
            market: market_addr(q),
            position: position_addr(q, user),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

pub fn trade_ix(user: &Pubkey, user_usdc: &Pubkey, usdc_mint: &Pubkey, q: u16, side: u8, quantity: u64, max_net_cost: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::Trade { side, quantity, max_net_cost }.data(),
        ngdp_futures::accounts::Trade {
            user: *user,
            market: market_addr(q),
            position: position_addr(q, user),
            vault: vault_addr(q),
            usdc_mint: *usdc_mint,
            user_usdc: *user_usdc,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

/// Open the position (if asked) and trade, in one transaction. No price limit.
pub fn do_trade(w: &mut World, user: &Keypair, user_usdc: &Pubkey, side: u8, quantity: u64, first: bool) -> bool {
    let mut ixs = vec![];
    if first {
        ixs.push(open_position_ix(&user.pubkey(), 4));
    }
    ixs.push(trade_ix(&user.pubkey(), user_usdc, &w.usdc_mint, 4, side, quantity, i64::MAX));
    send_many(&mut w.svm, &ixs, user)
}

pub fn claim_ix(user: &Pubkey, user_usdc: &Pubkey, usdc_mint: &Pubkey, q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::Claim {}.data(),
        ngdp_futures::accounts::Claim {
            user: *user,
            market: market_addr(q),
            position: position_addr(q, user),
            vault: vault_addr(q),
            usdc_mint: *usdc_mint,
            user_usdc: *user_usdc,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

pub fn withdraw_pool_ix(admin: &Pubkey, admin_usdc: &Pubkey, usdc_mint: &Pubkey, q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::WithdrawPool {}.data(),
        ngdp_futures::accounts::WithdrawPool {
            admin: *admin,
            config: pda(&[CONFIG_SEED]),
            market: market_addr(q),
            vault: vault_addr(q),
            usdc_mint: *usdc_mint,
            admin_usdc: *admin_usdc,
            token_program: token_program(),
        }
        .to_account_metas(None),
    )
}

pub fn report_ix(reporter: &Pubkey, q: u16, value_tenths: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::ReportValue { value_tenths, release_url: "https://www.bea.gov/news/example".to_string() }.data(),
        ngdp_futures::accounts::ReportValue { reporter: *reporter, config: pda(&[CONFIG_SEED]), market: market_addr(q) }
            .to_account_metas(None),
    )
}

pub fn correct_ix(admin: &Pubkey, q: u16, value_tenths: i64) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CorrectValue { value_tenths }.data(),
        ngdp_futures::accounts::CorrectValue { admin: *admin, config: pda(&[CONFIG_SEED]), market: market_addr(q) }
            .to_account_metas(None),
    )
}

pub fn finalise_ix(q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::Finalise {}.data(),
        ngdp_futures::accounts::Finalise { target_table: pda(&[TARGET_TABLE_SEED]), market: market_addr(q) }
            .to_account_metas(None),
    )
}

pub fn fallback_ix(q: u16) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::FallbackSettle {}.data(),
        ngdp_futures::accounts::FallbackSettle { market: market_addr(q) }.to_account_metas(None),
    )
}
