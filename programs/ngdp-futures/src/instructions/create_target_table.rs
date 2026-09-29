use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Config, TargetTable},
};

#[derive(Accounts)]
pub struct CreateTargetTable<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    /// `has_one = admin` checks the signer really is the admin stored in Config.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub config: Account<'info, Config>,
    /// Created once. There is deliberately no "update" instruction anywhere,
    /// so after this the targets are locked forever.
    #[account(
        init,
        payer = admin,
        space = TargetTable::SPACE,
        seeds = [TARGET_TABLE_SEED],
        bump
    )]
    pub target_table: Account<'info, TargetTable>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_target_table(ctx: Context<CreateTargetTable>) -> Result<()> {
    let table = &mut ctx.accounts.target_table;
    table.targets = LEVEL_TARGETS;
    table.bump = ctx.bumps.target_table;
    msg!(
        "Level Targets locked: {} quarters, Q3 2025 = {} tenths of $bn",
        NUM_TARGETS,
        table.targets[0]
    );
    Ok(())
}
