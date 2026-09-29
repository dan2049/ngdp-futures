//! One-time devnet setup. Safe to re-run: every step is skipped if already done.
//!
//!   cargo run --example setup_devnet
//!
//! 1. Creates a mock USDC mint (6 decimals) that YOU control, and mints you 100,000.
//! 2. initialize_config  (you = admin and reporter)
//! 3. create_target_table (the 41 locked Level Targets)
//! 4. create_market for Q3 2026 .. Q3 2027 with a +/- $1,000bn band.
//!
//! Uses your wallet at ~/.config/solana/id.json. Mock-USDC keys go in ./.devnet/ (gitignored).

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        system_program,
    },
    InstructionData, ToAccountMetas,
};
use ngdp_futures::constants::*;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use std::{fs, path::PathBuf};

const RPC_URL: &str = "https://api.devnet.solana.com";
const USDC: u64 = 1_000_000;

/// (quarter_index, label, scheduled BEA advance-estimate release, unix seconds UTC)
/// Q3 2026 is BEA's published date (29 Oct 2026, 8:30am ET). 2027 dates are estimates
/// (last Thursday of the following month); they only affect the 365-day fallback deadline.
const MARKETS: [(u16, &str, i64); 5] = [
    (4, "Q3 2026", 1_793_277_000), // Thu 29 Oct 2026
    (5, "Q4 2026", 1_801_143_000), // Thu 28 Jan 2027 (est.)
    (6, "Q1 2027", 1_809_001_800), // Thu 29 Apr 2027 (est.)
    (7, "Q2 2027", 1_816_864_200), // Thu 29 Jul 2027 (est.)
    (8, "Q3 2027", 1_824_726_600), // Thu 28 Oct 2027 (est.)
];
const FLOOR_BN: i64 = -1000;
const CAP_BN: i64 = 1000;

fn main() {
    let rpc = RpcClient::new(RPC_URL.to_string());
    let home = std::env::var("HOME").expect("HOME not set");
    let wallet = read_keypair(&PathBuf::from(&home).join(".config/solana/id.json"));
    let me = wallet.pubkey();
    println!("Wallet:  {}", me);
    println!("Balance: {:.3} SOL", rpc.get_balance(&me).unwrap() as f64 / 1e9);
    println!("Program: {}\n", ngdp_futures::id());

    fs::create_dir_all(".devnet").unwrap();
    let mint_kp = load_or_create_keypair(".devnet/usdc-mint.json");
    let usdc_acct_kp = load_or_create_keypair(".devnet/usdc-account.json");
    let usdc_mint = mint_kp.pubkey();

    // ---- 1. Mock USDC ----
    if exists(&rpc, &usdc_mint) {
        println!("[skip] mock USDC mint exists: {}", usdc_mint);
    } else {
        let rent = rpc.get_minimum_balance_for_rent_exemption(82).unwrap();
        let ixs = [
            create_account_ix(me, usdc_mint, rent, 82, token_program()),
            init_mint_ix(usdc_mint, me, 6),
        ];
        send(&rpc, &ixs, &wallet, &[&mint_kp], "create mock USDC mint");
    }
    let usdc_acct = usdc_acct_kp.pubkey();
    if exists(&rpc, &usdc_acct) {
        println!("[skip] your mock USDC account exists: {}", usdc_acct);
    } else {
        let rent = rpc.get_minimum_balance_for_rent_exemption(165).unwrap();
        let ixs = [
            create_account_ix(me, usdc_acct, rent, 165, token_program()),
            init_token_account_ix(usdc_acct, usdc_mint, me),
            mint_to_ix(usdc_mint, usdc_acct, me, 100_000 * USDC),
        ];
        send(&rpc, &ixs, &wallet, &[&usdc_acct_kp], "create your USDC account + mint 100,000 mock USDC");
    }

    // ---- 2. Config ----
    let config = pda(&[CONFIG_SEED]);
    if exists(&rpc, &config) {
        println!("[skip] config exists: {}", config);
    } else {
        let ix = Instruction::new_with_bytes(
            ngdp_futures::id(),
            &ngdp_futures::instruction::InitializeConfig { reporter: me, usdc_mint }.data(),
            ngdp_futures::accounts::InitializeConfig { admin: me, config, system_program: system_program::ID }
                .to_account_metas(None),
        );
        send(&rpc, &[ix], &wallet, &[], "initialize_config");
    }

    // ---- 3. Target table ----
    let target_table = pda(&[TARGET_TABLE_SEED]);
    if exists(&rpc, &target_table) {
        println!("[skip] target table exists: {}", target_table);
    } else {
        let ix = Instruction::new_with_bytes(
            ngdp_futures::id(),
            &ngdp_futures::instruction::CreateTargetTable {}.data(),
            ngdp_futures::accounts::CreateTargetTable {
                admin: me,
                config,
                target_table,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send(&rpc, &[ix], &wallet, &[], "create_target_table (41 Level Targets, locked)");
    }

    // ---- 4. Markets ----
    for (q, label, release) in MARKETS {
        let market = pda(&[MARKET_SEED, &q.to_le_bytes()]);
        if exists(&rpc, &market) {
            println!("[skip] market {} exists: {}", label, market);
            continue;
        }
        let ix = Instruction::new_with_bytes(
            ngdp_futures::id(),
            &ngdp_futures::instruction::CreateMarket {
                quarter_index: q,
                floor_bn: FLOOR_BN,
                cap_bn: CAP_BN,
                expected_release_ts: release,
            }
            .data(),
            ngdp_futures::accounts::CreateMarket {
                admin: me,
                config,
                target_table,
                market,
                above_mint: pda(&[ABOVE_MINT_SEED, market.as_ref()]),
                below_mint: pda(&[BELOW_MINT_SEED, market.as_ref()]),
                vault: pda(&[VAULT_SEED, market.as_ref()]),
                usdc_mint,
                token_program: token_program(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send(&rpc, &[ix], &wallet, &[], &format!("create_market {} (quarter {})", label, q));
    }

    let x = |a: &Pubkey| format!("https://explorer.solana.com/address/{}?cluster=devnet", a);
    println!("\nDone. Look it up on Solana Explorer:");
    println!("  Program:       {}", x(&ngdp_futures::id()));
    println!("  Target table:  {}", x(&target_table));
    println!("  Mock USDC:     {}", x(&usdc_mint));
    for (q, label, _) in MARKETS {
        println!("  Market {}: {}", label, x(&pda(&[MARKET_SEED, &q.to_le_bytes()])));
    }
}

// ---------- helpers ----------

fn token_program() -> Pubkey {
    anchor_spl::token::ID
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &ngdp_futures::id()).0
}

fn exists(rpc: &RpcClient, addr: &Pubkey) -> bool {
    rpc.get_account(addr).is_ok()
}

fn send(rpc: &RpcClient, ixs: &[Instruction], payer: &Keypair, extra: &[&Keypair], what: &str) {
    let blockhash = rpc.get_latest_blockhash().unwrap();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &signers).unwrap();
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!("[done] {}\n       https://explorer.solana.com/tx/{}?cluster=devnet", what, sig),
        Err(e) => panic!("[FAILED] {}: {}", what, e),
    }
}

/// Keypair files are a JSON list of 64 numbers (same format as the Solana CLI).
fn read_keypair(path: &PathBuf) -> Keypair {
    let text = fs::read_to_string(path).unwrap_or_else(|_| panic!("can't read {}", path.display()));
    let bytes: Vec<u8> = text
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|n| n.trim().parse().unwrap())
        .collect();
    Keypair::try_from(bytes.as_slice()).expect("bad keypair file")
}

fn load_or_create_keypair(path: &str) -> Keypair {
    let p = PathBuf::from(path);
    if p.exists() {
        return read_keypair(&p);
    }
    let kp = Keypair::new();
    let list: Vec<String> = kp.to_bytes().iter().map(|b| b.to_string()).collect();
    fs::write(&p, format!("[{}]", list.join(","))).unwrap();
    kp
}

// Raw System / Token program instructions (so we need no extra libraries).

fn create_account_ix(from: Pubkey, new: Pubkey, lamports: u64, space: u64, owner: Pubkey) -> Instruction {
    let mut data = 0u32.to_le_bytes().to_vec(); // CreateAccount
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: system_program::ID,
        accounts: vec![AccountMeta::new(from, true), AccountMeta::new(new, true)],
        data,
    }
}

fn init_mint_ix(mint: Pubkey, authority: Pubkey, decimals: u8) -> Instruction {
    let mut data = vec![20u8, decimals]; // InitializeMint2
    data.extend_from_slice(authority.as_ref());
    data.push(0); // no freeze authority
    Instruction { program_id: token_program(), accounts: vec![AccountMeta::new(mint, false)], data }
}

fn init_token_account_ix(account: Pubkey, mint: Pubkey, owner: Pubkey) -> Instruction {
    let mut data = vec![18u8]; // InitializeAccount3
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: token_program(),
        accounts: vec![AccountMeta::new(account, false), AccountMeta::new_readonly(mint, false)],
        data,
    }
}

fn mint_to_ix(mint: Pubkey, to: Pubkey, authority: Pubkey, amount: u64) -> Instruction {
    let mut data = vec![7u8]; // MintTo
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: token_program(),
        accounts: vec![
            AccountMeta::new(mint, false),
            AccountMeta::new(to, false),
            AccountMeta::new_readonly(authority, true),
        ],
        data,
    }
}
