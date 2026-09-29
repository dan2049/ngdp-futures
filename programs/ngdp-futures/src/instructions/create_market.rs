use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Config, Market, MarketState, TargetTable},
};

#[derive(Accounts)]
#[instruction(quarter_index: u16)]
pub struct CreateMarket<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ ErrorCode::Unauthorized,
        has_one = usdc_mint
    )]
    pub config: Box<Account<'info, Config>>,
    #[account(seeds = [TARGET_TABLE_SEED], bump = target_table.bump)]
    pub target_table: Box<Account<'info, TargetTable>>,
    /// One market per quarter: its address comes from the quarter number,
    /// so `init` fails if that quarter already has a market.
    #[account(
        init,
        payer = admin,
        space = 8 + Market::INIT_SPACE,
        seeds = [MARKET_SEED, &quarter_index.to_le_bytes()],
        bump
    )]
    pub market: Box<Account<'info, Market>>,
    /// ABOVE token. 0 decimals: you hold whole contracts. Only the market can mint.
    #[account(
        init,
        payer = admin,
        seeds = [ABOVE_MINT_SEED, market.key().as_ref()],
        bump,
        mint::decimals = 0,
        mint::authority = market
    )]
    pub above_mint: Box<Account<'info, Mint>>,
    #[account(
        init,
        payer = admin,
        seeds = [BELOW_MINT_SEED, market.key().as_ref()],
        bump,
        mint::decimals = 0,
        mint::authority = market
    )]
    pub below_mint: Box<Account<'info, Mint>>,
    /// The USDC vault. Only the market (i.e. this program's rules) can move it.
    #[account(
        init,
        payer = admin,
        seeds = [VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = market
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_market(
    ctx: Context<CreateMarket>,
    quarter_index: u16,
    floor_bn: i64,
    cap_bn: i64,
    expected_release_ts: i64,
) -> Result<()> {
    // Q3 2025 (index 0) is the base, not a tradable quarter.
    require!(quarter_index >= 1, ErrorCode::InvalidQuarter);
    require!(
        ctx.accounts.target_table.target_for(quarter_index).is_some(),
        ErrorCode::InvalidQuarter
    );
    require!(floor_bn < cap_bn, ErrorCode::InvalidBand);

    let width_bn = cap_bn.checked_sub(floor_bn).ok_or(ErrorCode::MathOverflow)? as u64;
    let collateral_per_pair = width_bn
        .checked_mul(USDC_UNITS_PER_BN)
        .ok_or(ErrorCode::MathOverflow)?;

    let market = &mut ctx.accounts.market;
    market.quarter_index = quarter_index;
    market.floor_bn = floor_bn;
    market.cap_bn = cap_bn;
    market.collateral_per_pair = collateral_per_pair;
    market.usdc_mint = ctx.accounts.usdc_mint.key();
    market.above_mint = ctx.accounts.above_mint.key();
    market.below_mint = ctx.accounts.below_mint.key();
    market.vault = ctx.accounts.vault.key();
    market.state = MarketState::Open;
    market.total_pairs = 0;
    market.bump = ctx.bumps.market;
    market.expected_release_ts = expected_release_ts;
    market.reported_value_tenths = 0;
    market.report_ts = 0;
    market.gap_tenths = 0;
    market.above_payout = 0;
    market.below_payout = 0;

    msg!(
        "Market created: quarter {}, band {}bn to {}bn, {} USDC units per pair",
        quarter_index,
        floor_bn,
        cap_bn,
        collateral_per_pair
    );
    Ok(())
}
