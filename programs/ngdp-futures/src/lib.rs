pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("5B6CZuXEbcah5BHcp7N8KxacPQsEhKLE3N4RweXNga8W");

/// NGDP Level Target futures. The public "menu" of instructions.
#[program]
pub mod ngdp_futures {
    use super::*;

    /// One-time setup: sets admin (the caller), reporter and USDC mint.
    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        reporter: Pubkey,
        usdc_mint: Pubkey,
    ) -> Result<()> {
        crate::instructions::initialize_config::handle_initialize_config(ctx, reporter, usdc_mint)
    }

    /// One-time, admin only: writes and locks the 41 Level Targets.
    pub fn create_target_table(ctx: Context<CreateTargetTable>) -> Result<()> {
        crate::instructions::create_target_table::handle_create_target_table(ctx)
    }

    /// Admin only: open a quarter's market with its band (e.g. -1000, +1000 $bn).
    pub fn create_market(
        ctx: Context<CreateMarket>,
        quarter_index: u16,
        floor_bn: i64,
        cap_bn: i64,
    ) -> Result<()> {
        crate::instructions::create_market::handle_create_market(ctx, quarter_index, floor_bn, cap_bn)
    }

    /// Anyone: deposit USDC, receive `amount` ABOVE + `amount` BELOW.
    pub fn mint_pair(ctx: Context<MintPair>, amount: u64) -> Result<()> {
        crate::instructions::mint_pair::handle_mint_pair(ctx, amount)
    }

    /// Anyone: return `amount` ABOVE + `amount` BELOW, get the USDC back.
    pub fn redeem_pair(ctx: Context<RedeemPair>, amount: u64) -> Result<()> {
        crate::instructions::redeem_pair::handle_redeem_pair(ctx, amount)
    }
}
