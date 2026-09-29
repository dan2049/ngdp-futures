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
}
