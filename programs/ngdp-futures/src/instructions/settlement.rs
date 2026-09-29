//! Getting the BEA number on-chain: report -> (correct) -> 24h -> finalise,
//! or the last-resort fallback if no estimate ever arrives.

use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::ErrorCode,
    state::{payouts_for_gap, Config, Market, MarketState, TargetTable},
};

// ---------- report_value (reporter) ----------

#[derive(Accounts)]
pub struct ReportValue<'info> {
    pub reporter: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = reporter @ ErrorCode::NotReporter)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Account<'info, Market>,
}

/// Post the first BEA estimate published for this quarter (normally the advance
/// estimate; if BEA skips it, whichever estimate it publishes first).
pub fn handle_report_value(ctx: Context<ReportValue>, value_tenths: i64, release_url: String) -> Result<()> {
    require!(release_url.len() <= MAX_URL_LEN, ErrorCode::UrlTooLong);
    require!(value_tenths > 0, ErrorCode::InvalidValue);
    let market = &mut ctx.accounts.market;
    require!(market.state == MarketState::Open, ErrorCode::MarketNotOpen);

    market.reported_value_tenths = value_tenths;
    market.report_ts = Clock::get()?.unix_timestamp;
    market.state = MarketState::Pending;
    // The URL lives in the permanent transaction log, where anyone can check it.
    msg!("Reported quarter {}: {} tenths of $bn. Source: {}", market.quarter_index, value_tenths, release_url);
    Ok(())
}

// ---------- correct_value (admin) ----------

#[derive(Accounts)]
pub struct CorrectValue<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ ErrorCode::Unauthorized)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Account<'info, Market>,
}

pub fn handle_correct_value(ctx: Context<CorrectValue>, value_tenths: i64) -> Result<()> {
    require!(value_tenths > 0, ErrorCode::InvalidValue);
    let market = &mut ctx.accounts.market;
    require!(market.state == MarketState::Pending, ErrorCode::NotPending);

    msg!("Corrected quarter {}: {} -> {} tenths of $bn", market.quarter_index, market.reported_value_tenths, value_tenths);
    market.reported_value_tenths = value_tenths;
    market.report_ts = Clock::get()?.unix_timestamp; // restart the 24 hours
    Ok(())
}

// ---------- finalise (anyone) ----------

#[derive(Accounts)]
pub struct Finalise<'info> {
    #[account(seeds = [TARGET_TABLE_SEED], bump = target_table.bump)]
    pub target_table: Account<'info, TargetTable>,
    #[account(mut, seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Account<'info, Market>,
}

pub fn handle_finalise(ctx: Context<Finalise>) -> Result<()> {
    let market = &mut ctx.accounts.market;
    require!(market.state == MarketState::Pending, ErrorCode::NotPending);
    let now = Clock::get()?.unix_timestamp;
    let window_ends = market.report_ts.checked_add(CHALLENGE_WINDOW_SECS).ok_or(ErrorCode::MathOverflow)?;
    require!(now >= window_ends, ErrorCode::ChallengeWindowOpen);

    let target = ctx
        .accounts
        .target_table
        .target_for(market.quarter_index)
        .ok_or(ErrorCode::InvalidQuarter)?;
    let gap = market.reported_value_tenths.checked_sub(target).ok_or(ErrorCode::MathOverflow)?;
    let (above, below) = payouts_for_gap(market.floor_bn, market.cap_bn, market.collateral_per_pair, gap)
        .ok_or(ErrorCode::MathOverflow)?;

    market.gap_tenths = gap;
    market.above_payout = above;
    market.below_payout = below;
    market.state = MarketState::Final;
    msg!(
        "Final quarter {}: NGDP {} vs target {} => gap {} tenths of $bn. ABOVE pays {}, BELOW pays {} USDC units",
        market.quarter_index, market.reported_value_tenths, target, gap, above, below
    );
    Ok(())
}

// ---------- fallback_settle (anyone) ----------

#[derive(Accounts)]
pub struct FallbackSettle<'info> {
    #[account(mut, seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Account<'info, Market>,
}

pub fn handle_fallback_settle(ctx: Context<FallbackSettle>) -> Result<()> {
    let market = &mut ctx.accounts.market;
    // Only if nothing was ever reported. A Pending market can always be finalised instead.
    require!(market.state == MarketState::Open, ErrorCode::MarketNotOpen);
    let now = Clock::get()?.unix_timestamp;
    let deadline = market.expected_release_ts.checked_add(FALLBACK_DELAY_SECS).ok_or(ErrorCode::MathOverflow)?;
    require!(now >= deadline, ErrorCode::FallbackTooEarly);

    let half = market.collateral_per_pair / 2;
    market.above_payout = half;
    market.below_payout = market.collateral_per_pair - half;
    market.state = MarketState::Fallback;
    msg!("Fallback settlement for quarter {}: each token pays {} USDC units", market.quarter_index, half);
    Ok(())
}
