use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

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
    /// One market per quarter: `init` fails if that quarter already has one.
    #[account(
        init,
        payer = admin,
        space = 8 + Market::INIT_SPACE,
        seeds = [MARKET_SEED, &quarter_index.to_le_bytes()],
        bump
    )]
    pub market: Box<Account<'info, Market>>,
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
    /// Where the pool's starting liquidity comes from.
    #[account(mut, token::mint = usdc_mint, token::authority = admin)]
    pub admin_usdc: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Open a quarter's market and fund its pool.
/// `liquidity` = USDC base units the admin puts in the pool.
/// `starting_gap_bn` = the price the pool opens at (e.g. 0, or -450).
pub fn handle_create_market(
    ctx: Context<CreateMarket>,
    quarter_index: u16,
    floor_bn: i64,
    cap_bn: i64,
    expected_release_ts: i64,
    liquidity: u64,
    starting_gap_bn: i64,
) -> Result<()> {
    require!(quarter_index >= 1, ErrorCode::InvalidQuarter);
    require!(
        ctx.accounts.target_table.target_for(quarter_index).is_some(),
        ErrorCode::InvalidQuarter
    );
    require!(floor_bn < cap_bn, ErrorCode::InvalidBand);
    let width = cap_bn.checked_sub(floor_bn).ok_or(ErrorCode::MathOverflow)? as u64;
    let collateral_per_pair = width.checked_mul(USDC_UNITS_PER_BN).ok_or(ErrorCode::MathOverflow)?;
    let set_units = collateral_per_pair / UNITS_PER_CONTRACT; // = width, e.g. 2000

    // Liquidity buys whole micro-sets, and at least one full contract each side.
    require!(liquidity > 0 && liquidity % set_units == 0, ErrorCode::InvalidLiquidity);
    let sets = (liquidity / set_units) as u128;
    require!(sets >= UNITS_PER_CONTRACT as u128, ErrorCode::InvalidLiquidity);

    // Starting price p = (gap - floor) / (cap - floor) must be strictly inside the band.
    require!(
        starting_gap_bn > floor_bn && starting_gap_bn < cap_bn,
        ErrorCode::InvalidStartingPrice
    );
    let num = (starting_gap_bn - floor_bn) as u128;
    let den = width as u128;
    // Pool price of the above side = below / (above + below). Keep `sets` of the scarcer
    // side... and hand the leftover of the other side to the funder (lp_*).
    let (pool_above, pool_below) = if 2 * num == den {
        (sets, sets)
    } else if 2 * num < den {
        (sets, sets * num / (den - num)) // gap starts below the middle: above is cheap
    } else {
        (sets * (den - num) / num, sets)
    };
    require!(pool_above > 0 && pool_below > 0, ErrorCode::InvalidLiquidity);

    // Move the liquidity into the vault.
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.admin_usdc.to_account_info(),
                mint: ctx.accounts.usdc_mint.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.admin.to_account_info(),
            },
        ),
        liquidity,
        ctx.accounts.usdc_mint.decimals,
    )?;

    let market = &mut ctx.accounts.market;
    market.quarter_index = quarter_index;
    market.floor_bn = floor_bn;
    market.cap_bn = cap_bn;
    market.collateral_per_pair = collateral_per_pair;
    market.usdc_mint = ctx.accounts.usdc_mint.key();
    market.vault = ctx.accounts.vault.key();
    market.state = MarketState::Open;
    market.bump = ctx.bumps.market;
    market.expected_release_ts = expected_release_ts;
    market.reported_value_tenths = 0;
    market.report_ts = 0;
    market.gap_tenths = 0;
    market.above_payout = 0;
    market.below_payout = 0;
    market.pool_above = pool_above as u64;
    market.pool_below = pool_below as u64;
    market.lp_above = (sets - pool_above) as u64;
    market.lp_below = (sets - pool_below) as u64;
    market.total_sets = sets as u64;
    market.last_trade_ts = 0;
    market.pool_withdrawn = false;

    msg!(
        "Market created: quarter {}, band {}bn to {}bn, pool funded with {} USDC units, opening at {}bn",
        quarter_index, floor_bn, cap_bn, liquidity, starting_gap_bn
    );
    Ok(())
}
