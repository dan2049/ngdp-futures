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

    /// Admin only: open a quarter's market with its band (e.g. -1000, +1000 $bn)
    /// and the scheduled BEA advance-estimate release time (unix seconds).
    pub fn create_market(
        ctx: Context<CreateMarket>,
        quarter_index: u16,
        floor_bn: i64,
        cap_bn: i64,
        expected_release_ts: i64,
    ) -> Result<()> {
        crate::instructions::create_market::handle_create_market(
            ctx,
            quarter_index,
            floor_bn,
            cap_bn,
            expected_release_ts,
        )
    }

    /// Anyone: deposit USDC, receive `amount` ABOVE + `amount` BELOW.
    pub fn mint_pair(ctx: Context<MintPair>, amount: u64) -> Result<()> {
        crate::instructions::mint_pair::handle_mint_pair(ctx, amount)
    }

    /// Anyone: return `amount` ABOVE + `amount` BELOW, get the USDC back.
    pub fn redeem_pair(ctx: Context<RedeemPair>, amount: u64) -> Result<()> {
        crate::instructions::redeem_pair::handle_redeem_pair(ctx, amount)
    }

    /// Reporter: post the first BEA estimate for the quarter (tenths of $bn) and its URL.
    /// Starts the 24-hour challenge window.
    pub fn report_value(ctx: Context<ReportValue>, value_tenths: i64, release_url: String) -> Result<()> {
        crate::instructions::settlement::handle_report_value(ctx, value_tenths, release_url)
    }

    /// Admin: replace a wrong reported value. Restarts the 24-hour window.
    pub fn correct_value(ctx: Context<CorrectValue>, value_tenths: i64) -> Result<()> {
        crate::instructions::settlement::handle_correct_value(ctx, value_tenths)
    }

    /// Anyone, 24h after the last report/correction: lock the value and compute payouts.
    pub fn finalise(ctx: Context<Finalise>) -> Result<()> {
        crate::instructions::settlement::handle_finalise(ctx)
    }

    /// Anyone, 365 days after the scheduled release with nothing reported: settle at the midpoint.
    pub fn fallback_settle(ctx: Context<FallbackSettle>) -> Result<()> {
        crate::instructions::settlement::handle_fallback_settle(ctx)
    }

    /// Token holders, after settlement: burn ABOVE and/or BELOW for their USDC payout.
    pub fn settle_redeem(ctx: Context<SettleRedeem>, above_amount: u64, below_amount: u64) -> Result<()> {
        crate::instructions::settle_redeem::handle_settle_redeem(ctx, above_amount, below_amount)
    }

    /// Anyone: post an offer to sell or buy `quantity` ABOVE at `price` USDC units each.
    /// The ABOVE (selling) or USDC (buying) is locked in escrow until filled or cancelled.
    pub fn post_offer(
        ctx: Context<PostOffer>,
        offer_id: u64,
        side: OfferSide,
        price: u64,
        quantity: u64,
    ) -> Result<()> {
        crate::instructions::offers::handle_post_offer(ctx, offer_id, side, price, quantity)
    }

    /// Anyone except the maker: take all or part of an offer.
    pub fn fill_offer(ctx: Context<FillOffer>, quantity: u64) -> Result<()> {
        crate::instructions::offers::handle_fill_offer(ctx, quantity)
    }

    /// Maker only: take back whatever is left in escrow and close the offer. Works in any market state.
    pub fn cancel_offer(ctx: Context<CancelOffer>) -> Result<()> {
        crate::instructions::offers::handle_cancel_offer(ctx)
    }
}
