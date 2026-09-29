use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Only the admin can do this")]
    Unauthorized,
    #[msg("Quarter must be between 1 (Q4 2025) and 40 (Q3 2035)")]
    InvalidQuarter,
    #[msg("Floor must be below cap")]
    InvalidBand,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("This market is not open for minting or redeeming pairs")]
    MarketNotOpen,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Only the reporter can do this")]
    NotReporter,
    #[msg("Market is not waiting on a reported value")]
    NotPending,
    #[msg("The 24-hour challenge window has not ended yet")]
    ChallengeWindowOpen,
    #[msg("The fallback deadline has not passed yet")]
    FallbackTooEarly,
    #[msg("Market is not settled yet")]
    NotSettled,
    #[msg("Reported NGDP must be positive")]
    InvalidValue,
    #[msg("Release URL is too long")]
    UrlTooLong,
    #[msg("Price must be above zero and no more than the collateral per pair")]
    InvalidPrice,
    #[msg("Token account or mint does not match this offer")]
    WrongMint,
    #[msg("You cannot fill your own offer")]
    SelfFill,
    #[msg("Not enough left on this offer")]
    NotEnoughRemaining,
}
