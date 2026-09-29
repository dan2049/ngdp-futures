//! The offer board: ABOVE traded against USDC, one board per market.
//! Makers lock what they're offering in an escrow account owned by the offer;
//! takers fill all or part; makers can cancel and get the rest back.
//! "Go short" = mint_pair + fill_offer (on a BuyAbove offer) in ONE transaction.

use anchor_lang::prelude::*;
use anchor_spl::token::{
    close_account, transfer_checked, CloseAccount, Mint, Token, TokenAccount, TransferChecked,
};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Market, MarketState, Offer, OfferSide},
};

// ---------- post_offer ----------

#[derive(Accounts)]
#[instruction(offer_id: u64)]
pub struct PostOffer<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()], bump = market.bump)]
    pub market: Box<Account<'info, Market>>,
    #[account(
        init,
        payer = maker,
        space = 8 + Offer::INIT_SPACE,
        seeds = [OFFER_SEED, market.key().as_ref(), maker.key().as_ref(), &offer_id.to_le_bytes()],
        bump
    )]
    pub offer: Box<Account<'info, Offer>>,
    /// Escrow is owned by the offer, so only this program's rules can release it.
    #[account(
        init,
        payer = maker,
        seeds = [ESCROW_SEED, offer.key().as_ref()],
        bump,
        token::mint = escrow_mint,
        token::authority = offer
    )]
    pub escrow: Box<Account<'info, TokenAccount>>,
    /// ABOVE mint if selling, USDC mint if buying (checked below).
    pub escrow_mint: Box<Account<'info, Mint>>,
    /// Where the maker's ABOVE or USDC comes from.
    #[account(mut, token::mint = escrow_mint, token::authority = maker)]
    pub maker_source: Box<Account<'info, TokenAccount>>,
    /// Where the maker's proceeds go: USDC if selling, ABOVE if buying (checked below).
    #[account(token::authority = maker)]
    pub maker_receive: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_post_offer(
    ctx: Context<PostOffer>,
    offer_id: u64,
    side: OfferSide,
    price: u64,
    quantity: u64,
) -> Result<()> {
    let market = &ctx.accounts.market;
    require!(market.state == MarketState::Open, ErrorCode::MarketNotOpen);
    require!(quantity > 0, ErrorCode::ZeroAmount);
    // ABOVE can never be worth more than a whole pair, or less than nothing.
    require!(price > 0 && price <= market.collateral_per_pair, ErrorCode::InvalidPrice);

    let (escrow_mint, receive_mint, escrow_amount) = match side {
        OfferSide::SellAbove => (market.above_mint, market.usdc_mint, quantity),
        OfferSide::BuyAbove => (
            market.usdc_mint,
            market.above_mint,
            price.checked_mul(quantity).ok_or(ErrorCode::MathOverflow)?,
        ),
    };
    require_keys_eq!(ctx.accounts.escrow_mint.key(), escrow_mint, ErrorCode::WrongMint);
    require_keys_eq!(ctx.accounts.maker_receive.mint, receive_mint, ErrorCode::WrongMint);

    // Lock the maker's side in escrow.
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.maker_source.to_account_info(),
                mint: ctx.accounts.escrow_mint.to_account_info(),
                to: ctx.accounts.escrow.to_account_info(),
                authority: ctx.accounts.maker.to_account_info(),
            },
        ),
        escrow_amount,
        ctx.accounts.escrow_mint.decimals,
    )?;

    let offer = &mut ctx.accounts.offer;
    offer.market = ctx.accounts.market.key();
    offer.maker = ctx.accounts.maker.key();
    offer.maker_receive = ctx.accounts.maker_receive.key();
    offer.escrow = ctx.accounts.escrow.key();
    offer.side = side;
    offer.price = price;
    offer.quantity = quantity;
    offer.offer_id = offer_id;
    offer.bump = ctx.bumps.offer;
    msg!("Offer posted: {} ABOVE at {} USDC units", quantity, price);
    Ok(())
}

// ---------- fill_offer ----------

#[derive(Accounts)]
pub struct FillOffer<'info> {
    pub taker: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, &market.quarter_index.to_le_bytes()],
        bump = market.bump,
        has_one = above_mint,
        has_one = usdc_mint
    )]
    pub market: Box<Account<'info, Market>>,
    #[account(
        mut,
        seeds = [OFFER_SEED, market.key().as_ref(), offer.maker.as_ref(), &offer.offer_id.to_le_bytes()],
        bump = offer.bump,
        has_one = market,
        has_one = escrow,
        has_one = maker_receive
    )]
    pub offer: Box<Account<'info, Offer>>,
    #[account(mut)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub maker_receive: Box<Account<'info, TokenAccount>>,
    pub above_mint: Box<Account<'info, Mint>>,
    pub usdc_mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = usdc_mint, token::authority = taker)]
    pub taker_usdc: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = above_mint, token::authority = taker)]
    pub taker_above: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_fill_offer(ctx: Context<FillOffer>, quantity: u64) -> Result<()> {
    require!(ctx.accounts.market.state == MarketState::Open, ErrorCode::MarketNotOpen);
    require!(quantity > 0, ErrorCode::ZeroAmount);
    require!(quantity <= ctx.accounts.offer.quantity, ErrorCode::NotEnoughRemaining);
    // Stops someone printing fake trades against themselves to move the headline price.
    require_keys_neq!(ctx.accounts.taker.key(), ctx.accounts.offer.maker, ErrorCode::SelfFill);

    let price = ctx.accounts.offer.price;
    let cost = price.checked_mul(quantity).ok_or(ErrorCode::MathOverflow)?;
    let token_program = ctx.accounts.token_program.key();

    // The offer "signs" to release escrow.
    let market_key = ctx.accounts.market.key();
    let maker_key = ctx.accounts.offer.maker;
    let id = ctx.accounts.offer.offer_id.to_le_bytes();
    let bump = [ctx.accounts.offer.bump];
    let offer_seeds: &[&[u8]] = &[OFFER_SEED, market_key.as_ref(), maker_key.as_ref(), &id, &bump];
    let signer = &[offer_seeds];

    match ctx.accounts.offer.side {
        OfferSide::SellAbove => {
            // Taker pays USDC to the maker...
            transfer_checked(
                CpiContext::new(
                    token_program,
                    TransferChecked {
                        from: ctx.accounts.taker_usdc.to_account_info(),
                        mint: ctx.accounts.usdc_mint.to_account_info(),
                        to: ctx.accounts.maker_receive.to_account_info(),
                        authority: ctx.accounts.taker.to_account_info(),
                    },
                ),
                cost,
                ctx.accounts.usdc_mint.decimals,
            )?;
            // ...and gets ABOVE from escrow.
            transfer_checked(
                CpiContext::new_with_signer(
                    token_program,
                    TransferChecked {
                        from: ctx.accounts.escrow.to_account_info(),
                        mint: ctx.accounts.above_mint.to_account_info(),
                        to: ctx.accounts.taker_above.to_account_info(),
                        authority: ctx.accounts.offer.to_account_info(),
                    },
                    signer,
                ),
                quantity,
                ctx.accounts.above_mint.decimals,
            )?;
        }
        OfferSide::BuyAbove => {
            // Taker gives ABOVE to the maker...
            transfer_checked(
                CpiContext::new(
                    token_program,
                    TransferChecked {
                        from: ctx.accounts.taker_above.to_account_info(),
                        mint: ctx.accounts.above_mint.to_account_info(),
                        to: ctx.accounts.maker_receive.to_account_info(),
                        authority: ctx.accounts.taker.to_account_info(),
                    },
                ),
                quantity,
                ctx.accounts.above_mint.decimals,
            )?;
            // ...and gets USDC from escrow.
            transfer_checked(
                CpiContext::new_with_signer(
                    token_program,
                    TransferChecked {
                        from: ctx.accounts.escrow.to_account_info(),
                        mint: ctx.accounts.usdc_mint.to_account_info(),
                        to: ctx.accounts.taker_usdc.to_account_info(),
                        authority: ctx.accounts.offer.to_account_info(),
                    },
                    signer,
                ),
                cost,
                ctx.accounts.usdc_mint.decimals,
            )?;
        }
    }

    ctx.accounts.offer.quantity -= quantity; // can't underflow: checked above
    let market = &mut ctx.accounts.market;
    market.last_price = price;
    market.last_trade_ts = Clock::get()?.unix_timestamp;
    msg!("Filled {} ABOVE at {} USDC units", quantity, price);
    Ok(())
}

// ---------- cancel_offer ----------

#[derive(Accounts)]
pub struct CancelOffer<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    /// `close = maker` deletes the offer account and returns its rent to the maker.
    #[account(
        mut,
        close = maker,
        seeds = [OFFER_SEED, offer.market.as_ref(), maker.key().as_ref(), &offer.offer_id.to_le_bytes()],
        bump = offer.bump,
        has_one = maker,
        has_one = escrow
    )]
    pub offer: Box<Account<'info, Offer>>,
    #[account(mut)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    #[account(address = escrow.mint @ ErrorCode::WrongMint)]
    pub escrow_mint: Box<Account<'info, Mint>>,
    #[account(mut, token::mint = escrow_mint, token::authority = maker)]
    pub maker_refund: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_cancel_offer(ctx: Context<CancelOffer>) -> Result<()> {
    let token_program = ctx.accounts.token_program.key();
    let market_key = ctx.accounts.offer.market;
    let maker_key = ctx.accounts.maker.key();
    let id = ctx.accounts.offer.offer_id.to_le_bytes();
    let bump = [ctx.accounts.offer.bump];
    let offer_seeds: &[&[u8]] = &[OFFER_SEED, market_key.as_ref(), maker_key.as_ref(), &id, &bump];
    let signer = &[offer_seeds];

    let remaining = ctx.accounts.escrow.amount;
    if remaining > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                token_program,
                TransferChecked {
                    from: ctx.accounts.escrow.to_account_info(),
                    mint: ctx.accounts.escrow_mint.to_account_info(),
                    to: ctx.accounts.maker_refund.to_account_info(),
                    authority: ctx.accounts.offer.to_account_info(),
                },
                signer,
            ),
            remaining,
            ctx.accounts.escrow_mint.decimals,
        )?;
    }
    // Close the now-empty escrow; its rent goes back to the maker.
    close_account(CpiContext::new_with_signer(
        token_program,
        CloseAccount {
            account: ctx.accounts.escrow.to_account_info(),
            destination: ctx.accounts.maker.to_account_info(),
            authority: ctx.accounts.offer.to_account_info(),
        },
        signer,
    ))?;
    msg!("Offer cancelled; {} returned to maker", remaining);
    Ok(())
}
