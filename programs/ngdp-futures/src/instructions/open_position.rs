use anchor_lang::prelude::*;

use crate::{
    constants::*,
    state::{Market, Position},
};

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Box<Account<'info, Market>>,
    /// One position per trader per market. The website adds this automatically on a first trade.
    #[account(
        init,
        payer = user,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, market.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub position: Box<Account<'info, Position>>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_position(ctx: Context<OpenPosition>) -> Result<()> {
    let position = &mut ctx.accounts.position;
    position.owner = ctx.accounts.user.key();
    position.market = ctx.accounts.market.key();
    position.above = 0;
    position.below = 0;
    position.net_paid = 0;
    position.bump = ctx.bumps.position;
    Ok(())
}
