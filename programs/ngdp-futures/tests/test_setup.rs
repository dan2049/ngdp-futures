//! Tests for step 1: Config + locked Level Target table.
//! Run with: anchor test

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

/// Start a fresh simulated chain with our program loaded.
fn setup() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/ngdp_futures.so"
    ));
    svm.add_program(ngdp_futures::id(), bytes).unwrap();
    svm
}

/// Sign and send one instruction. Returns true if it succeeded.
fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    svm.expire_blockhash(); // so a repeated transaction isn't rejected as a duplicate
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).is_ok()
}

fn pda(seed: &[u8]) -> Pubkey {
    Pubkey::find_program_address(&[seed], &ngdp_futures::id()).0
}

fn initialize_config_ix(admin: Pubkey, reporter: Pubkey, usdc_mint: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::InitializeConfig { reporter, usdc_mint }.data(),
        ngdp_futures::accounts::InitializeConfig {
            admin,
            config: pda(ngdp_futures::constants::CONFIG_SEED),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn create_target_table_ix(admin: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        ngdp_futures::id(),
        &ngdp_futures::instruction::CreateTargetTable {}.data(),
        ngdp_futures::accounts::CreateTargetTable {
            admin,
            config: pda(ngdp_futures::constants::CONFIG_SEED),
            target_table: pda(ngdp_futures::constants::TARGET_TABLE_SEED),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Every hard-coded target must equal 31,098.0 x 1.05^(n/4), rounded to $0.1bn.
#[test]
fn level_targets_match_formula() {
    for (n, &stored) in ngdp_futures::constants::LEVEL_TARGETS.iter().enumerate() {
        let expected = (31_098.0_f64 * 1.05_f64.powf(n as f64 / 4.0) * 10.0).round() as i64;
        assert_eq!(stored, expected, "target for quarter n = {} is wrong", n);
    }
}

#[test]
fn config_and_target_table() {
    let mut svm = setup();
    let admin = Keypair::new();
    let stranger = Keypair::new();
    svm.airdrop(&admin.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let reporter = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();

    // 1. Admin creates the Config.
    assert!(send(&mut svm, initialize_config_ix(admin.pubkey(), reporter, usdc_mint), &admin));
    let acct = svm.get_account(&pda(ngdp_futures::constants::CONFIG_SEED)).unwrap();
    let config = ngdp_futures::state::Config::try_deserialize(&mut acct.data.as_slice()).unwrap();
    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.reporter, reporter);
    assert_eq!(config.usdc_mint, usdc_mint);

    // 2. Nobody can create the Config a second time.
    assert!(!send(&mut svm, initialize_config_ix(stranger.pubkey(), reporter, usdc_mint), &stranger));

    // 3. A non-admin cannot create the target table.
    assert!(!send(&mut svm, create_target_table_ix(stranger.pubkey()), &stranger));

    // 4. The admin can, and the stored values are exactly the table.
    assert!(send(&mut svm, create_target_table_ix(admin.pubkey()), &admin));
    let acct = svm.get_account(&pda(ngdp_futures::constants::TARGET_TABLE_SEED)).unwrap();
    let table = ngdp_futures::state::TargetTable::try_deserialize(&mut acct.data.as_slice()).unwrap();
    assert_eq!(table.targets, ngdp_futures::constants::LEVEL_TARGETS);
    assert_eq!(table.target_for(0), Some(310_980)); // Q3 2025 = $31,098.0bn
    assert_eq!(table.target_for(5), Some(330_536)); // Q4 2026 = $33,053.6bn
    assert_eq!(table.target_for(40), Some(506_554)); // Q3 2035 = $50,655.4bn
    assert_eq!(table.target_for(41), None); // beyond the table

    // 5. The table is locked: even the admin can't create it again.
    assert!(!send(&mut svm, create_target_table_ix(admin.pubkey()), &admin));
}
