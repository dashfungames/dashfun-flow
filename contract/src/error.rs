use solana_program::program_error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    ZeroInput = 1,
    ZeroOutput = 2,
    InsufficientLiquidity = 3,
    Overflow = 4,
    InvalidInstruction = 5,
    InvalidMint = 6,
    InvalidDecimals = 7,
    InvalidAccount = 8,
}

impl From<BridgeError> for ProgramError {
    fn from(err: BridgeError) -> Self {
        ProgramError::Custom(err as u32)
    }
}
