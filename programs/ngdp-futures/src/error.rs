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
    #[msg("This market is closed for trading")]
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
    #[msg("Side must be buy (0) or sell (1)")]
    InvalidSide,
    #[msg("The price moved past your limit; nothing was traded")]
    SlippageExceeded,
    #[msg("Liquidity must be at least one contract set and a whole number of sets")]
    InvalidLiquidity,
    #[msg("Starting price must be strictly inside the band")]
    InvalidStartingPrice,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("The pool has already been withdrawn")]
    AlreadyWithdrawn,
}
