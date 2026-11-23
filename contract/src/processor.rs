use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed, set_return_data},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};

use crate::error::BridgeError;
use crate::instruction::{decode_instruction, encode_quote_result, Instruction};
use crate::quote::{quote, Direction, ZYRA_DECIMALS};
use crate::swap::{available_after_reserve, settle};

pub const STATE_SEED: &[u8] = b"zyra-state";
pub const SOL_VAULT_SEED: &[u8] = b"zyra-sol-vault";
pub const STATE_LEN: u64 = 33;

const TOKEN_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

const SPL_TRANSFER: u8 = 3;

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match decode_instruction(instruction_data)? {
        Instruction::Quote {
            direction,
            input_amount,
        } => {
            let priced = quote(input_amount, direction)?;
            set_return_data(&encode_quote_result(&priced));
            Ok(())
        }
        Instruction::Swap {
            direction,
            input_amount,
        } => process_swap(program_id, accounts, direction, input_amount),
        Instruction::Initialize => process_initialize(program_id, accounts),
    }
}

fn process_initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts = &mut accounts.iter();
    let payer = next_account_info(accounts)?;
    let state = next_account_info(accounts)?;
    let sol_vault = next_account_info(accounts)?;
    let zyra_mint = next_account_info(accounts)?;
    let system_program = next_account_info(accounts)?;
    if accounts.next().is_some() {
        return Err(BridgeError::InvalidAccount.into());
    }
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if system_program.key != &solana_program::system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    require_zyra_mint(zyra_mint)?;

    let (state_pda, state_bump) = Pubkey::find_program_address(&[STATE_SEED], program_id);
    let (vault_pda, vault_bump) = Pubkey::find_program_address(&[SOL_VAULT_SEED], program_id);
    if state.key != &state_pda || sol_vault.key != &vault_pda {
        return Err(BridgeError::InvalidAccount.into());
    }

    let rent = Rent::get()?;
    let state_bump_seed = [state_bump];
    let vault_bump_seed = [vault_bump];
    let state_seeds: &[&[u8]] = &[STATE_SEED, &state_bump_seed];
    let vault_seeds: &[&[u8]] = &[SOL_VAULT_SEED, &vault_bump_seed];

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            state.key,
            rent.minimum_balance(STATE_LEN as usize),
            STATE_LEN,
            program_id,
        ),
        &[payer.clone(), state.clone(), system_program.clone()],
        &[state_seeds],
    )?;
    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            sol_vault.key,
            rent.minimum_balance(0),
            0,
            program_id,
        ),
        &[payer.clone(), sol_vault.clone(), system_program.clone()],
        &[vault_seeds],
    )?;

    let mut data = state.try_borrow_mut_data()?;
    data[..32].copy_from_slice(zyra_mint.key.as_ref());
    data[32] = state_bump;
    Ok(())
}

fn process_swap(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    direction: Direction,
    input_amount: u64,
) -> ProgramResult {
    let accounts = &mut accounts.iter();
    let user = next_account_info(accounts)?;
    let state = next_account_info(accounts)?;
    let sol_vault = next_account_info(accounts)?;
    let zyra_mint = next_account_info(accounts)?;
    let zyra_vault = next_account_info(accounts)?;
    let user_zyra = next_account_info(accounts)?;
    let token_program = next_account_info(accounts)?;
    let system_program = next_account_info(accounts)?;
    if accounts.next().is_some() {
        return Err(BridgeError::InvalidAccount.into());
    }

    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !user.is_writable
        || !sol_vault.is_writable
        || !zyra_vault.is_writable
        || !user_zyra.is_writable
    {
        return Err(BridgeError::InvalidAccount.into());
    }
    if token_program.key != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if system_program.key != &solana_program::system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if user.key == sol_vault.key || user_zyra.key == zyra_vault.key {
        return Err(BridgeError::InvalidAccount.into());
    }

    let (state_pda, state_bump) = Pubkey::find_program_address(&[STATE_SEED], program_id);
    let (vault_pda, _vault_bump) = Pubkey::find_program_address(&[SOL_VAULT_SEED], program_id);
    if state.key != &state_pda || state.owner != program_id || sol_vault.key != &vault_pda {
        return Err(BridgeError::InvalidAccount.into());
    }
    if sol_vault.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    let state_data = state.try_borrow_data()?;
    if state_data.len() != STATE_LEN as usize || state_data[32] != state_bump {
        return Err(BridgeError::InvalidAccount.into());
    }
    if &state_data[..32] != zyra_mint.key.as_ref() {
        return Err(BridgeError::InvalidMint.into());
    }
    drop(state_data);
    require_zyra_mint(zyra_mint)?;

    let vault_token = read_token_account(zyra_vault)?;
    let user_token = read_token_account(user_zyra)?;
    if vault_token.mint != *zyra_mint.key || user_token.mint != *zyra_mint.key {
        return Err(BridgeError::InvalidMint.into());
    }
    if vault_token.owner != *state.key || user_token.owner != *user.key {
        return Err(BridgeError::InvalidAccount.into());
    }

    let rent = Rent::get()?;
    let output_budget = match direction {
        Direction::SolToZyra => vault_token.amount,
        Direction::ZyraToSol => {
            available_after_reserve(sol_vault.lamports(), rent.minimum_balance(0))
        }
    };

    let priced = settle(input_amount, direction, output_budget)?;
    let state_bump_seed = [state_bump];
    let state_seeds: &[&[u8]] = &[STATE_SEED, &state_bump_seed];

    match direction {
        Direction::SolToZyra => {
            invoke(
                &system_instruction::transfer(user.key, sol_vault.key, input_amount),
                &[user.clone(), sol_vault.clone(), system_program.clone()],
            )?;
            transfer_zyra(
                zyra_vault,
                user_zyra,
                state,
                priced.output_amount,
                Some(state_seeds),
            )?;
        }
        Direction::ZyraToSol => {
            transfer_zyra(user_zyra, zyra_vault, user, input_amount, None)?;
            pay_sol(sol_vault, user, priced.output_amount)?;
        }
    }
    Ok(())
}

fn require_zyra_mint(mint: &AccountInfo) -> ProgramResult {
    if mint.owner != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let data = mint.try_borrow_data()?;
    if data.len() < 82 || data[45] != 1 {
        return Err(BridgeError::InvalidMint.into());
    }
    if data[44] != ZYRA_DECIMALS {
        return Err(BridgeError::InvalidDecimals.into());
    }
    Ok(())
}

struct TokenAccount {
    mint: Pubkey,
    owner: Pubkey,
    amount: u64,
}

fn read_token_account(account: &AccountInfo) -> Result<TokenAccount, ProgramError> {
    if account.owner != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let data = account.try_borrow_data()?;
    if data.len() < 165 {
        return Err(BridgeError::InvalidAccount.into());
    }
    let mut amount = [0u8; 8];
    amount.copy_from_slice(&data[64..72]);
    Ok(TokenAccount {
        mint: read_pubkey(&data[0..32]),
        owner: read_pubkey(&data[32..64]),
        amount: u64::from_le_bytes(amount),
    })
}

fn read_pubkey(bytes: &[u8]) -> Pubkey {
    let mut key = [0u8; 32];
    key.copy_from_slice(bytes);
    Pubkey::new_from_array(key)
}

fn transfer_zyra<'a>(
    source: &AccountInfo<'a>,
    destination: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    amount: u64,
    signer_seeds: Option<&[&[u8]]>,
) -> ProgramResult {
    let mut data = [0u8; 9];
    data[0] = SPL_TRANSFER;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    let ix = solana_program::instruction::Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            solana_program::instruction::AccountMeta::new(*source.key, false),
            solana_program::instruction::AccountMeta::new(*destination.key, false),
            solana_program::instruction::AccountMeta::new_readonly(*authority.key, true),
        ],
        data: data.to_vec(),
    };
    let infos = [source.clone(), destination.clone(), authority.clone()];
    match signer_seeds {
        Some(seeds) => invoke_signed(&ix, &infos, &[seeds]),
        None => invoke(&ix, &infos),
    }
}

fn pay_sol(sol_vault: &AccountInfo, user: &AccountInfo, lamports: u64) -> ProgramResult {
    let mut vault_lamports = sol_vault.try_borrow_mut_lamports()?;
    let mut user_lamports = user.try_borrow_mut_lamports()?;
    let vault_next = (**vault_lamports)
        .checked_sub(lamports)
        .ok_or(ProgramError::InsufficientFunds)?;
    let user_next = (**user_lamports)
        .checked_add(lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **vault_lamports = vault_next;
    **user_lamports = user_next;
    Ok(())
}
