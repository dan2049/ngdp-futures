pub mod amm;
pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("5B6CZuXEbcah5BHcp7N8KxacPQsEhKLE3N4RweXNga8W");

/// NGDP Level Target futures: buy or sell the expected gap, priced by a pool.
#[program]
pub mod ngdp_futures {
    use super::*;

    /// One-time setup: sets admin (the caller), reporter and USDC mint.
    pub fn initialize_config(ctx: Context<InitializeConfig>, reporter: Pubkey, usdc_mint: Pubkey) -> Result<()> {
        crate::instructions::initialize_config::handle_initialize_config(ctx, reporter, usdc_mint)
    }

    /// One-time, admin only: writes and locks the 41 Level Targets.
    pub fn create_target_table(ctx: Context<CreateTargetTable>) -> Result<()> {
        crate::instructions::create_target_table::handle_create_target_table(ctx)
    }

    /// Admin only: open a quarter's market and fund its pool.
    pub fn create_market(
        ctx: Context<CreateMarket>,
        quarter_index: u16,
        floor_bn: i64,
        cap_bn: i64,
        expected_release_ts: i64,
        liquidity: u64,
        starting_gap_bn: i64,
    ) -> Result<()> {
        crate::instructions::create_market::handle_create_market(
            ctx, quarter_index, floor_bn, cap_bn, expected_release_ts, liquidity, starting_gap_bn,
        )
    }

    /// Anyone: create your position record for a market (once).
    pub fn open_position(ctx: Context<OpenPosition>) -> Result<()> {
        crate::instructions::open_position::handle_open_position(ctx)
    }

    /// Anyone: buy (0) or sell (1) contracts against the pool, with a price limit.
    pub fn trade(ctx: Context<Trade>, side: u8, quantity: u64, max_net_cost: i64) -> Result<()> {
        crate::instructions::trade::handle_trade(ctx, side, quantity, max_net_cost)
    }

    /// Reporter: post the first BEA estimate for the quarter. Starts the 24-hour window.
    pub fn report_value(ctx: Context<ReportValue>, value_tenths: i64, release_url: String) -> Result<()> {
        crate::instructions::settlement::handle_report_value(ctx, value_tenths, release_url)
    }

    /// Admin: replace a wrong reported value. Restarts the 24-hour window.
    pub fn correct_value(ctx: Context<CorrectValue>, value_tenths: i64) -> Result<()> {
        crate::instructions::settlement::handle_correct_value(ctx, value_tenths)
    }

    /// Anyone, 24h after the last report/correction: lock the value and set payouts.
    pub fn finalise(ctx: Context<Finalise>) -> Result<()> {
        crate::instructions::settlement::handle_finalise(ctx)
    }

    /// Anyone, 365 days after the scheduled release with nothing reported: settle at the midpoint.
    pub fn fallback_settle(ctx: Context<FallbackSettle>) -> Result<()> {
        crate::instructions::settlement::handle_fallback_settle(ctx)
    }

    /// Anyone with a position, after settlement: collect the payout.
    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        crate::instructions::claim::handle_claim(ctx)
    }

    /// Admin, after settlement: take out what the pool's holdings are worth.
    pub fn withdraw_pool(ctx: Context<WithdrawPool>) -> Result<()> {
        crate::instructions::withdraw_pool::handle_withdraw_pool(ctx)
    }
}
