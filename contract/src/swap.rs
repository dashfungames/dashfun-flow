use crate::error::BridgeError;
use crate::quote::{quote, Direction, Quote};

pub fn available_after_reserve(balance: u64, reserve: u64) -> u64 {
    balance.saturating_sub(reserve)
}

pub fn settle(
    input_amount: u64,
    direction: Direction,
    output_budget: u64,
) -> Result<Quote, BridgeError> {
    if input_amount == 0 {
        return Err(BridgeError::ZeroInput);
    }
    let priced = quote(input_amount, direction)?;
    if priced.output_amount == 0 {
        return Err(BridgeError::ZeroOutput);
    }
    if output_budget < priced.output_amount {
        return Err(BridgeError::InsufficientLiquidity);
    }
    Ok(priced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quote::{LAMPORTS_PER_SOL, ZYRA_BASE_UNITS_PER_TOKEN};

    #[test]
    fn swap_pays_the_quote_when_the_vault_can_cover_it() {
        let input = LAMPORTS_PER_SOL;
        let priced = quote(input, Direction::SolToZyra).unwrap();
        let settled = settle(input, Direction::SolToZyra, priced.output_amount).unwrap();
        assert_eq!(settled, priced);
        assert_eq!(settled.output_amount, 99_700_000_000);
    }

    #[test]
    fn quote_can_succeed_when_the_vault_cannot_pay() {
        let input = 2_500 * ZYRA_BASE_UNITS_PER_TOKEN;
        let priced = quote(input, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.output_amount, 24_925_000_000);

        let err = settle(input, Direction::ZyraToSol, priced.output_amount - 1).unwrap_err();
        assert_eq!(err, BridgeError::InsufficientLiquidity);
    }

    #[test]
    fn rent_reserve_shrinks_the_sol_budget_before_settle() {
        let vault_lamports = 2_000_000_000;
        let rent = 1_000_000_000;
        let budget = available_after_reserve(vault_lamports, rent);
        assert_eq!(budget, 1_000_000_000);

        let input = 100 * ZYRA_BASE_UNITS_PER_TOKEN;
        let settled = settle(input, Direction::ZyraToSol, budget).unwrap();
        assert_eq!(settled.output_amount, 997_000_000);

        let too_big = 2_500 * ZYRA_BASE_UNITS_PER_TOKEN;
        let err = settle(too_big, Direction::ZyraToSol, budget).unwrap_err();
        assert_eq!(err, BridgeError::InsufficientLiquidity);
    }

    #[test]
    fn swap_rejects_a_rounded_zero_output() {
        let priced = quote(99, Direction::ZyraToSol).unwrap();
        assert_eq!(priced.output_amount, 0);
        let err = settle(99, Direction::ZyraToSol, u64::MAX).unwrap_err();
        assert_eq!(err, BridgeError::ZeroOutput);
    }

    #[test]
    fn swap_rejects_zero_input() {
        assert_eq!(
            settle(0, Direction::SolToZyra, u64::MAX).unwrap_err(),
            BridgeError::ZeroInput
        );
    }

    #[test]
    fn one_hundred_zyra_base_units_settle_to_one_lamport() {
        let settled = settle(100, Direction::ZyraToSol, 1).unwrap();
        assert_eq!(settled.fee_amount, 0);
        assert_eq!(settled.output_amount, 1);
    }
}
