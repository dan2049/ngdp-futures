use anchor_lang::prelude::*;
use anchor_spl::token::{mint_to, transfer_checked, Mint, MintTo, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Market, MarketState},
};

#[derive(Accounts)]
pub struct MintPair<'info> {
    pub user: Signer<'info>,
    /// has_one checks each account passed in really belongs to this market.
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
    /// The user's own USDC, ABOVE and BELOW token accounts.
    #[account(mut, token::mint = usdc_mint, token::authority = user)]
    pub user_usdc: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = above_mint, token::authority = user)]
    pub user_above: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = below_mint, token::authority = user)]
    pub user_below: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

/// Deposit `amount` x collateral_per_pair USDC; receive `amount` ABOVE + `amount` BELOW.
pub fn handle_mint_pair(ctx: Context<MintPair>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::ZeroAmount);
    require!(
        ctx.accounts.market.state == MarketState::Open,
        ErrorCode::MarketNotOpen
    );
    let usdc_amount = amount
        .checked_mul(ctx.accounts.market.collateral_per_pair)
        .ok_or(ErrorCode::MathOverflow)?;
    let token_program = ctx.accounts.token_program.key();

    // 1. User's USDC -> vault (the user signs this).
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
        usdc_amount,
        ctx.accounts.usdc_mint.decimals,
    )?;

    // 2. Mint ABOVE and BELOW to the user (the market "signs" with its seeds).
    let quarter = ctx.accounts.market.quarter_index.to_le_bytes();
    let bump = [ctx.accounts.market.bump];
    let market_seeds: &[&[u8]] = &[MARKET_SEED, &quarter, &bump];
    let signer = &[market_seeds];

    mint_to(
        CpiContext::new_with_signer(
            token_program,
            MintTo {
                mint: ctx.accounts.above_mint.to_account_info(),
                to: ctx.accounts.user_above.to_account_info(),
                authority: ctx.accounts.market.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;
    mint_to(
        CpiContext::new_with_signer(
            token_program,
            MintTo {
                mint: ctx.accounts.below_mint.to_account_info(),
                to: ctx.accounts.user_below.to_account_info(),
                authority: ctx.accounts.market.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;

    let market = &mut ctx.accounts.market;
    market.total_pairs = market
        .total_pairs
        .checked_add(amount)
        .ok_or(ErrorCode::MathOverflow)?;
    msg!("Minted {} pairs for {} USDC units", amount, usdc_amount);
    Ok(())
}
