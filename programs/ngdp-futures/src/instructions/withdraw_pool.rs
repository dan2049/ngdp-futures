use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Config, Market, MarketState},
};

#[derive(Accounts)]
pub struct WithdrawPool<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ ErrorCode::Unauthorized)]
    pub config: Box<Account<'info, Config>>,
    #[account(
        mut,
        seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()],
        bump = market.bump,
        has_one = vault,
        has_one = usdc_mint
    )]
    pub market: Box<Account<'info, Market>>,
    #[account(mut)]
    pub vault: Box<Account<'info, TokenAccount>>,
    pub usdc_mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = usdc_mint, token::authority = admin)]
    pub admin_usdc: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

/// After settlement: the pool funder takes out what the pool's holdings are worth.
pub fn handle_withdraw_pool(ctx: Context<WithdrawPool>) -> Result<()> {
    let m = &ctx.accounts.market;
    require!(
        m.state == MarketState::Final || m.state == MarketState::Fallback,
        ErrorCode::NotSettled
    );
    require!(!m.pool_withdrawn, ErrorCode::AlreadyWithdrawn);
    let above = m.pool_above as u128 + m.lp_above as u128;
    let below = m.pool_below as u128 + m.lp_below as u128;
    let payout = (above * m.above_payout as u128 + below * m.below_payout as u128) / UNITS_PER_CONTRACT as u128;
    let payout = u64::try_from(payout).map_err(|_| ErrorCode::MathOverflow)?;

    if payout > 0 {
        let quarter = m.quarter_index.to_le_bytes();
        let bump = [m.bump];
        let market_seeds: &[&[u8]] = &[MARKET_SEED, &quarter, &bump];
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.vault.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.admin_usdc.to_account_info(),
                    authority: ctx.accounts.market.to_account_info(),
                },
                &[market_seeds],
            ),
            payout,
            ctx.accounts.usdc_mint.decimals,
        )?;
    }
    let market = &mut ctx.accounts.market;
    market.pool_above = 0;
    market.pool_below = 0;
    market.lp_above = 0;
    market.lp_below = 0;
    market.pool_withdrawn = true;
    msg!("Pool withdrawn: {} USDC units", payout);
    Ok(())
}
