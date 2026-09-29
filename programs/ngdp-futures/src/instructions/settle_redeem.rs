use anchor_lang::prelude::*;
use anchor_spl::token::{burn, transfer_checked, Burn, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Market, MarketState},
};

#[derive(Accounts)]
pub struct SettleRedeem<'info> {
    pub user: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()],
        bump = market.bump,
        has_one = above_mint,
        has_one = below_mint,
        has_one = vault,
        has_one = usdc_mint
    )]
    pub market: Box<Account<'info, Market>>,
    #[account(mut)]
    pub above_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub below_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub vault: Box<Account<'info, TokenAccount>>,
    pub usdc_mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = usdc_mint, token::authority = user)]
    pub user_usdc: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = above_mint, token::authority = user)]
    pub user_above: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = below_mint, token::authority = user)]
    pub user_below: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

/// After settlement: burn ABOVE and/or BELOW, receive their payouts in USDC.
/// ABOVE and BELOW can be redeemed separately, by different people.
pub fn handle_settle_redeem(ctx: Context<SettleRedeem>, above_amount: u64, below_amount: u64) -> Result<()> {
    require!(above_amount > 0 || below_amount > 0, ErrorCode::ZeroAmount);
    let state = ctx.accounts.market.state;
    require!(
        state == MarketState::Final || state == MarketState::Fallback,
        ErrorCode::NotSettled
    );
    let payout = above_amount
        .checked_mul(ctx.accounts.market.above_payout)
        .and_then(|a| below_amount.checked_mul(ctx.accounts.market.below_payout).and_then(|b| a.checked_add(b)))
        .ok_or(ErrorCode::MathOverflow)?;
    let token_program = ctx.accounts.token_program.key();

    if above_amount > 0 {
        burn(
            CpiContext::new(
                token_program,
                Burn {
                    mint: ctx.accounts.above_mint.to_account_info(),
                    from: ctx.accounts.user_above.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            above_amount,
        )?;
    }
    if below_amount > 0 {
        burn(
            CpiContext::new(
                token_program,
                Burn {
                    mint: ctx.accounts.below_mint.to_account_info(),
                    from: ctx.accounts.user_below.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            below_amount,
        )?;
    }

    if payout > 0 {
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
            payout,
            ctx.accounts.usdc_mint.decimals,
        )?;
    }
    msg!("Settled {} ABOVE + {} BELOW for {} USDC units", above_amount, below_amount, payout);
    Ok(())
}
