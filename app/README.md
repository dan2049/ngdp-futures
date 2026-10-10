# NGDP Level Target Futures – website

A single static page (`index.html`) that reads the devnet markets and trades through Phantom.

## Run it on your Mac

    cd ~/code/ngdp-futures/app
    python3 -m http.server 8000

Then open http://localhost:8000 in the browser where Phantom is installed.
Phantom must be in Testnet Mode on **Solana Devnet**.

## Give a wallet test funds

    cd ~/code/ngdp-futures
    cargo run --example faucet -p ngdp-futures -- <WALLET_ADDRESS> 10000

Sends 10,000 mock USDC and tops the wallet up to 0.2 SOL for fees.
