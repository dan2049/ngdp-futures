use anchor_lang::prelude::*;

use crate::{constants::*, state::Config};

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    /// Whoever calls this becomes the admin, and pays the account's rent.
    #[account(mut)]
    pub admin: Signer<'info>,
    /// `init` creates the account and fails if it already exists,
    /// so this can only ever run once.
    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    reporter: Pubkey,
    usdc_mint: Pubkey,
) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.reporter = reporter;
    config.usdc_mint = usdc_mint;
    config.bump = ctx.bumps.config;
    msg!("Config created. Admin: {}", config.admin);
    Ok(())
}
