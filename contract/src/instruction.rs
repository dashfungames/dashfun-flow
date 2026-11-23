use crate::error::BridgeError;
use crate::quote::{Direction, Quote};

pub const QUOTE_TAG: u8 = 0;
pub const SWAP_TAG: u8 = 1;
pub const INITIALIZE_TAG: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instruction {
    Quote {
        direction: Direction,
        input_amount: u64,
    },
    Swap {
        direction: Direction,
        input_amount: u64,
    },
    Initialize,
}

pub fn decode_instruction(data: &[u8]) -> Result<Instruction, BridgeError> {
    let (tag, rest) = data.split_first().ok_or(BridgeError::InvalidInstruction)?;
    match *tag {
        QUOTE_TAG | SWAP_TAG => {
            if rest.len() != 9 {
                return Err(BridgeError::InvalidInstruction);
            }
            let direction = Direction::from_byte(rest[0])?;
            let mut amount_bytes = [0u8; 8];
            amount_bytes.copy_from_slice(&rest[1..9]);
            let input_amount = u64::from_le_bytes(amount_bytes);
            if *tag == QUOTE_TAG {
                Ok(Instruction::Quote {
                    direction,
                    input_amount,
                })
            } else {
                Ok(Instruction::Swap {
                    direction,
                    input_amount,
                })
            }
        }
        INITIALIZE_TAG if rest.is_empty() => Ok(Instruction::Initialize),
        _ => Err(BridgeError::InvalidInstruction),
    }
}

pub fn encode_quote_result(priced: &Quote) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[0..8].copy_from_slice(&priced.fee_amount.to_le_bytes());
    out[8..16].copy_from_slice(&priced.net_amount.to_le_bytes());
    out[16..24].copy_from_slice(&priced.output_amount.to_le_bytes());
    out
}

pub fn decode_quote_result(data: &[u8]) -> Result<Quote, BridgeError> {
    if data.len() != 24 {
        return Err(BridgeError::InvalidInstruction);
    }
    let mut fee = [0u8; 8];
    let mut net = [0u8; 8];
    let mut output = [0u8; 8];
    fee.copy_from_slice(&data[0..8]);
    net.copy_from_slice(&data[8..16]);
    output.copy_from_slice(&data[16..24]);
    Ok(Quote {
        fee_amount: u64::from_le_bytes(fee),
        net_amount: u64::from_le_bytes(net),
        output_amount: u64::from_le_bytes(output),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quote::{quote, LAMPORTS_PER_SOL};

    #[test]
    fn quote_return_data_round_trips() {
        let priced = quote(LAMPORTS_PER_SOL, Direction::SolToZyra).unwrap();
        let encoded = encode_quote_result(&priced);
        assert_eq!(decode_quote_result(&encoded).unwrap(), priced);
    }

    #[test]
    fn quote_and_swap_payloads_share_a_layout() {
        let mut data = [0u8; 10];
        data[0] = SWAP_TAG;
        data[1] = Direction::ZyraToSol as u8;
        data[2..10].copy_from_slice(&(2_500u64).to_le_bytes());
        match decode_instruction(&data).unwrap() {
            Instruction::Swap {
                direction,
                input_amount,
            } => {
                assert_eq!(direction, Direction::ZyraToSol);
                assert_eq!(input_amount, 2_500);
            }
            other => panic!("expected swap, got {other:?}"),
        }

        data[0] = QUOTE_TAG;
        match decode_instruction(&data).unwrap() {
            Instruction::Quote { input_amount, .. } => assert_eq!(input_amount, 2_500),
            other => panic!("expected quote, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_direction_and_truncated_data() {
        assert_eq!(
            decode_instruction(&[QUOTE_TAG, 9, 1, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(),
            BridgeError::InvalidInstruction
        );
        assert_eq!(
            decode_instruction(&[QUOTE_TAG]).unwrap_err(),
            BridgeError::InvalidInstruction
        );
        assert_eq!(
            decode_instruction(&[]).unwrap_err(),
            BridgeError::InvalidInstruction
        );
    }
}
