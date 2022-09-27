//! Error types

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use solana_program::{
    decode_error::DecodeError, msg, program_error::PrintProgramError, program_error::ProgramError,
};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum DistError {
    /// Invalid instruction
    #[error("Invalid instruction")]
    InvalidInstruction,

    /// The account cannot be initialized because it is already in use
    #[error("Account is already initialized")]
    AlreadyInitialized,

    /// Distribution has already started
    #[error("Distribution has already started")]
    DistributionAlreadyStarted,

    /// Token initialize account failed
    #[error("Token initialize account failed")]
    TokenInitializeAccountFailed,

    /// Token transfer failed
    #[error("Token transfer failed")]
    TokenTransferFailed,

    /// Dist authority doesn't match
    #[error("Unauthorized dist authority")]
    UnauthorizedDistAuthority,

    /// Token account's owner does not match
    #[error("Token account owner mismatch")]
    TokenAccountOwnerMismatch,

    /// Too many recipients
    #[error("Too many recipients")]
    TooManyRecipients,
}

impl From<DistError> for ProgramError {
    fn from(e: DistError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for DistError {
    fn type_of() -> &'static str {
        "DistError"
    }
}

impl PrintProgramError for DistError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        msg!(&self.to_string());
    }
}
