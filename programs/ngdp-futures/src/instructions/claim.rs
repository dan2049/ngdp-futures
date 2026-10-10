use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Market, MarketState, Position},
};

#[derive(Accounts)]
pub struct Claim<'info> {
    pub user: Signer<'info>,
    #[account(
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

/// After settlement: pay out the position (longs get the above payout, shorts the below payout).
pub fn handle_claim(ctx: Context<Claim>) -> Result<()> {
    let m = &ctx.accounts.market;
    require!(
        m.state == MarketState::Final || m.state == MarketState::Fallback,
        ErrorCode::NotSettled
    );
    let p = &ctx.accounts.position;
    require!(p.above > 0 || p.below > 0, ErrorCode::NothingToClaim);
    let payout = (p.above as u128 * m.above_payout as u128 + p.below as u128 * m.below_payout as u128)
        / UNITS_PER_CONTRACT as u128;
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
                    to: ctx.accounts.user_usdc.to_account_info(),
                    authority: ctx.accounts.market.to_account_info(),
                },
                &[market_seeds],
            ),
            payout,
            ctx.accounts.usdc_mint.decimals,
        )?;
    }
    let position = &mut ctx.accounts.position;
    position.above = 0;
    position.below = 0;
    position.net_paid = position.net_paid.checked_sub(payout as i64).ok_or(ErrorCode::MathOverflow)?;
    msg!("Claimed {} USDC units", payout);
    Ok(())
}
