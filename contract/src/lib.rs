pub mod error;
pub mod instruction;
pub mod processor;
pub mod quote;
pub mod swap;

pub use error::BridgeError;
pub use quote::{
    quote, Direction, Quote, LAMPORTS_PER_SOL, ZYRA_BASE_UNITS_PER_TOKEN, ZYRA_DECIMALS,
    ZYRA_MAX_SUPPLY_TOKENS,
};
pub use swap::settle;

#[cfg(all(target_os = "solana", not(feature = "no-entrypoint")))]
solana_program::entrypoint!(crate::processor::process_instruction);
