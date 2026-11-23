use crate::error::BridgeError;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    SolToZyra = 0,
    ZyraToSol = 1,
}

impl Direction {
    pub fn from_byte(byte: u8) -> Result<Self, BridgeError> {
        match byte {
            0 => Ok(Self::SolToZyra),
            1 => Ok(Self::ZyraToSol),
            _ => Err(BridgeError::InvalidInstruction),
        }
    }
}

pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
pub const ZYRA_BASE_UNITS_PER_TOKEN: u64 = 1_000_000_000;
pub const ZYRA_DECIMALS: u8 = 9;
pub const ZYRA_MAX_SUPPLY_TOKENS: u64 = 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub fee_amount: u64,
    pub net_amount: u64,
    pub output_amount: u64,
}

pub fn quote(input_amount: u64, direction: Direction) -> Result<Quote, BridgeError> {
    let fee_amount = ((input_amount as u128) * 30 / 10_000) as u64;
    let net_amount = input_amount
        .checked_sub(fee_amount)
        .ok_or(BridgeError::Overflow)?;
    let output_amount = match direction {
        Direction::SolToZyra => {
            u64::try_from((net_amount as u128) * 100).map_err(|_| BridgeError::Overflow)?
        }
        Direction::ZyraToSol => ((net_amount as u128) / 100) as u64,
    };
    Ok(Quote {
        fee_amount,
        net_amount,
        output_amount,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sol_quotes_ninety_nine_point_seven_zyra() {
        let priced = quote(LAMPORTS_PER_SOL, Direction::SolToZyra).unwrap();
        assert_eq!(priced.fee_amount, 3_000_000);
        assert_eq!(priced.net_amount, 997_000_000);
        assert_eq!(priced.output_amount, 99_700_000_000);
    }

    #[test]
    fn two_thousand_five_hundred_zyra_quotes_twenty_four_point_nine_two_five_sol() {
        let input = 2_500 * ZYRA_BASE_UNITS_PER_TOKEN;
        let priced = quote(input, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.fee_amount, 7_500_000_000);
        assert_eq!(priced.net_amount, 2_492_500_000_000);
        assert_eq!(priced.output_amount, 24_925_000_000);
        assert_ne!(priced.output_amount, 997_000_000);
    }

    #[test]
    fn fee_fraction_matches_in_both_directions() {
        let input = 10 * LAMPORTS_PER_SOL;
        let to_zyra = quote(input, Direction::SolToZyra).unwrap();
        let to_sol = quote(input, Direction::ZyraToSol).unwrap();
        assert_eq!(to_zyra.fee_amount, input * 30 / 10_000);
        assert_eq!(to_zyra.fee_amount, to_sol.fee_amount);
        assert_eq!(to_zyra.net_amount, to_sol.net_amount);
        assert_eq!(to_zyra.output_amount, to_zyra.net_amount * 100);
        assert_eq!(to_sol.output_amount, to_sol.net_amount / 100);
    }

    #[test]
    fn fee_is_applied_before_the_rate() {
        let priced = quote(333, Direction::SolToZyra).unwrap();
        assert_eq!(priced.fee_amount, 0);
        assert_eq!(priced.output_amount, 33_300);

        let priced = quote(10_000, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.fee_amount, 30);
        assert_eq!(priced.net_amount, 9_970);
        assert_eq!(priced.output_amount, 99);
    }

    #[test]
    fn dust_sol_still_quotes_zyra_because_the_fee_floors_to_zero() {
        let priced = quote(1, Direction::SolToZyra).unwrap();
        assert_eq!(priced.fee_amount, 0);
        assert_eq!(priced.net_amount, 1);
        assert_eq!(priced.output_amount, 100);
    }

    #[test]
    fn zyra_below_one_hundred_base_units_quotes_zero_lamports() {
        let priced = quote(99, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.fee_amount, 0);
        assert_eq!(priced.output_amount, 0);

        let priced = quote(100, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.fee_amount, 0);
        assert_eq!(priced.net_amount, 100);
        assert_eq!(priced.output_amount, 1);
    }

    #[test]
    fn zero_input_quotes_zero_and_does_not_error() {
        let priced = quote(0, Direction::SolToZyra).unwrap();
        assert_eq!(priced.fee_amount, 0);
        assert_eq!(priced.net_amount, 0);
        assert_eq!(priced.output_amount, 0);
    }

    #[test]
    fn supply_cap_is_not_the_exchange_rate() {
        assert_eq!(ZYRA_MAX_SUPPLY_TOKENS, 1_000_000_000);
        let priced = quote(LAMPORTS_PER_SOL, Direction::SolToZyra).unwrap();
        assert_eq!(priced.output_amount, 99_700_000_000);
    }

    #[test]
    fn sol_to_zyra_rejects_an_output_that_does_not_fit_in_u64() {
        let err = quote(u64::MAX, Direction::SolToZyra).unwrap_err();
        assert_eq!(err, BridgeError::Overflow);
        assert!(quote(u64::MAX, Direction::ZyraToSol).is_ok());
    }
}
