use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    amm,
    constants::*,
    error::ErrorCode,
    state::{Market, MarketState, Position},
};

#[derive(Accounts)]
pub struct Trade<'info> {
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()],
        bump = market.bump,
        has_one = vault,
        has_one = usdc_mint
    )]
    pub market: Box<Account<'info, Market>>,
    #[account(
        mut,
        seeds = [POSITION_SEED, market.key().as_ref(), user.key().as_ref()],
        bump = position.bump,
        has_one = market
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(mut)]
    pub vault: Box<Account<'info, TokenAccount>>,
    pub usdc_mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = usdc_mint, token::authority = user)]
    pub user_usdc: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

/// Buy (side 0) or sell (side 1) `quantity` millionths of a contract against the pool.
///
/// Buying first closes any short you hold, then opens/extends a long. Selling does the
/// reverse. `max_net_cost` is your limit in USDC base units: the most you'll pay overall,
/// or, if negative, the least you'll accept to receive. If the price has moved past it,
/// nothing happens.
pub fn handle_trade(ctx: Context<Trade>, side: u8, quantity: u64, max_net_cost: i64) -> Result<()> {
    require!(quantity > 0, ErrorCode::ZeroAmount);
    require!(side == SIDE_BUY || side == SIDE_SELL, ErrorCode::InvalidSide);
    require!(ctx.accounts.market.state == MarketState::Open, ErrorCode::MarketNotOpen);

    let m = &ctx.accounts.market;
    let set_units = m.set_units() as i128;
    let mut pool_above = m.pool_above as u128;
    let mut pool_below = m.pool_below as u128;
    let mut long = ctx.accounts.position.above as u128;
    let mut short = ctx.accounts.position.below as u128;
    let mut remaining = quantity as u128;
    let mut sets_delta: i128 = 0; // + pairs created (trader pays), - pairs retired (trader receives)

    if side == SIDE_BUY {
        // 1. Close a short: hand the pool back "below" units, receive collateral.
        let close = remaining.min(short);
        if close > 0 {
            let (n, nb, na) = amm::sell_exact(pool_below, pool_above, close).ok_or(ErrorCode::MathOverflow)?;
            pool_below = nb;
            pool_above = na;
            short -= close;
            sets_delta -= n as i128;
            remaining -= close;
        }
        // 2. Open a long: pay for new pairs, receive "above" units from the pool.
        if remaining > 0 {
            let (n, na, nb) = amm::buy_exact(pool_above, pool_below, remaining).ok_or(ErrorCode::MathOverflow)?;
            pool_above = na;
            pool_below = nb;
            long += remaining;
            sets_delta += n as i128;
        }
    } else {
        // 1. Close a long.
        let close = remaining.min(long);
        if close > 0 {
            let (n, na, nb) = amm::sell_exact(pool_above, pool_below, close).ok_or(ErrorCode::MathOverflow)?;
            pool_above = na;
            pool_below = nb;
            long -= close;
            sets_delta -= n as i128;
            remaining -= close;
        }
        // 2. Open a short.
        if remaining > 0 {
            let (n, nb, na) = amm::buy_exact(pool_below, pool_above, remaining).ok_or(ErrorCode::MathOverflow)?;
            pool_below = nb;
            pool_above = na;
            short += remaining;
            sets_delta += n as i128;
        }
    }

    // Net cash: positive = trader pays the vault, negative = vault pays the trader.
    let cash = sets_delta.checked_mul(set_units).ok_or(ErrorCode::MathOverflow)?;
    require!(cash <= max_net_cost as i128, ErrorCode::SlippageExceeded);

    let token_program = ctx.accounts.token_program.key();
    let decimals = ctx.accounts.usdc_mint.decimals;
    if cash > 0 {
        transfer_checked(
            CpiContext::new(
                token_program,
                TransferChecked {
                    from: ctx.accounts.user_usdc.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            cash as u64,
            decimals,
        )?;
    } else if cash < 0 {
        let quarter = ctx.accounts.market.quarter_index.to_le_bytes();
        let bump = [ctx.accounts.market.bump];
        let market_seeds: &[&[u8]] = &[MARKET_SEED, &quarter, &bump];
        transfer_checked(
            CpiContext::new_with_signer(
                token_program,
                TransferChecked {
                    from: ctx.accounts.vault.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.user_usdc.to_account_info(),
                    authority: ctx.accounts.market.to_account_info(),
                },
                &[market_seeds],
            ),
            (-cash) as u64,
            decimals,
        )?;
    }

    let position = &mut ctx.accounts.position;
    position.above = u64::try_from(long).map_err(|_| ErrorCode::MathOverflow)?;
    position.below = u64::try_from(short).map_err(|_| ErrorCode::MathOverflow)?;
    position.net_paid = position
        .net_paid
        .checked_add(i64::try_from(cash).map_err(|_| ErrorCode::MathOverflow)?)
        .ok_or(ErrorCode::MathOverflow)?;

    let market = &mut ctx.accounts.market;
    market.pool_above = u64::try_from(pool_above).map_err(|_| ErrorCode::MathOverflow)?;
    market.pool_below = u64::try_from(pool_below).map_err(|_| ErrorCode::MathOverflow)?;
    let total = (market.total_sets as i128).checked_add(sets_delta).ok_or(ErrorCode::MathOverflow)?;
    market.total_sets = u64::try_from(total).map_err(|_| ErrorCode::MathOverflow)?;
    market.last_trade_ts = Clock::get()?.unix_timestamp;

    msg!(
        "{} {} units; net cash {} USDC units; pool now {} above / {} below",
        if side == SIDE_BUY { "Bought" } else { "Sold" },
        quantity, cash, market.pool_above, market.pool_below
    );
    Ok(())
}
