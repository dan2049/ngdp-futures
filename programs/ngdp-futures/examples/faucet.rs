//! Send mock USDC (and a little SOL for fees) to any wallet, e.g. your Phantom address.
//!
//!   cargo run --example faucet -p ngdp-futures -- <WALLET_ADDRESS> [USDC_AMOUNT]
//!
//! Default 10,000 mock USDC. Tops the wallet up to 0.2 SOL if it has less.
//! Only works for whoever holds the mock-USDC mint authority (your ~/.config/solana/id.json).

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        system_program,
    },
};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use std::{fs, path::PathBuf, str::FromStr};

const RPC_URL: &str = "https://api.devnet.solana.com";
const ATA_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const TARGET_SOL_LAMPORTS: u64 = 200_000_000; // 0.2 SOL

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --example faucet -p ngdp-futures -- <WALLET_ADDRESS> [USDC_AMOUNT]");
        std::process::exit(1);
    }
    let recipient = Pubkey::from_str(&args[1]).expect("not a valid Solana address");
    let usdc_whole: u64 = args.get(2).map(|a| a.parse().expect("amount must be a whole number")).unwrap_or(10_000);

    let rpc = RpcClient::new(RPC_URL.to_string());
    let home = std::env::var("HOME").unwrap();
    let wallet = read_keypair(&PathBuf::from(home).join(".config/solana/id.json"));
    let mint = read_keypair(&PathBuf::from(".devnet/usdc-mint.json")).pubkey();
    let token_program = anchor_spl::token::ID;
    let ata_program = Pubkey::from_str(ATA_PROGRAM).unwrap();
    let ata = Pubkey::find_program_address(
        &[recipient.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ata_program,
    )
    .0;

    let mut ixs = vec![
        // Create the recipient's mock-USDC account if it doesn't exist yet.
        Instruction {
            program_id: ata_program,
            accounts: vec![
                AccountMeta::new(wallet.pubkey(), true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(recipient, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(token_program, false),
            ],
            data: vec![1], // CreateIdempotent
        },
        // Mint mock USDC into it.
        Instruction {
            program_id: token_program,
            accounts: vec![
                AccountMeta::new(mint, false),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet.pubkey(), true),
            ],
            data: [vec![7u8], (usdc_whole * 1_000_000).to_le_bytes().to_vec()].concat(), // MintTo
        },
    ];
    let balance = rpc.get_balance(&recipient).unwrap_or(0);
    if balance < TARGET_SOL_LAMPORTS {
        let top_up = TARGET_SOL_LAMPORTS - balance;
        ixs.push(Instruction {
            program_id: system_program::ID,
            accounts: vec![AccountMeta::new(wallet.pubkey(), true), AccountMeta::new(recipient, false)],
            data: [2u32.to_le_bytes().to_vec(), top_up.to_le_bytes().to_vec()].concat(), // Transfer
        });
        println!("Topping up {:.3} SOL for fees", top_up as f64 / 1e9);
    }

    let blockhash = rpc.get_latest_blockhash().unwrap();
    let msg = Message::new_with_blockhash(&ixs, Some(&wallet.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&wallet]).unwrap();
    let sig = rpc.send_and_confirm_transaction(&tx).expect("faucet transaction failed");
    println!("Sent {} mock USDC to {}", usdc_whole, recipient);
    println!("https://explorer.solana.com/tx/{}?cluster=devnet", sig);
}

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
