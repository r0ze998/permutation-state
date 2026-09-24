//! The two SPL Token instructions the program needs, built by hand so the
//! program carries no token-crate version pins (SPL Token v3 layout).

use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    pubkey::Pubkey,
};

pub const TOKEN_PROGRAM_ID: Pubkey = solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
/// Size of an SPL token account.
pub const TOKEN_ACCOUNT_LEN: usize = 165;

/// `InitializeAccount3 { owner }` (tag 18).
pub fn initialize_account3<'a>(account: &AccountInfo<'a>, mint: &AccountInfo<'a>, owner: &Pubkey) -> ProgramResult {
    let mut data = Vec::with_capacity(33);
    data.push(18);
    data.extend_from_slice(owner.as_ref());
    let ix = Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![AccountMeta::new(*account.key, false), AccountMeta::new_readonly(*mint.key, false)],
        data,
    };
    invoke(&ix, &[account.clone(), mint.clone()])
}

/// `TransferChecked { amount, decimals }` (tag 12). `seeds` signs for a PDA authority.
#[allow(clippy::too_many_arguments)]
pub fn transfer_checked<'a>(
    source: &AccountInfo<'a>,
    mint: &AccountInfo<'a>,
    destination: &AccountInfo<'a>,
    authority: &AccountInfo<'a>,
    amount: u64,
    decimals: u8,
    seeds: Option<&[&[u8]]>,
) -> ProgramResult {
    let mut data = Vec::with_capacity(10);
    data.push(12);
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(decimals);
    let ix = Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*source.key, false),
            AccountMeta::new_readonly(*mint.key, false),
            AccountMeta::new(*destination.key, false),
            AccountMeta::new_readonly(*authority.key, true),
        ],
        data,
    };
    let infos = [source.clone(), mint.clone(), destination.clone(), authority.clone()];
    match seeds {
        Some(s) => invoke_signed(&ix, &infos, &[s]),
        None => invoke(&ix, &infos),
    }
}

/// Mint decimals from a mint account (SPL layout: byte 44).
pub fn mint_decimals(mint: &AccountInfo) -> Option<u8> {
    if mint.owner != &TOKEN_PROGRAM_ID {
        return None;
    }
    mint.try_borrow_data().ok()?.get(44).copied()
}

/// (mint, owner) of an SPL token account (layout: mint 0..32, owner 32..64).
pub fn token_account_mint_owner(account: &AccountInfo) -> Option<(Pubkey, Pubkey)> {
    if account.owner != &TOKEN_PROGRAM_ID {
        return None;
    }
    let data = account.try_borrow_data().ok()?;
    if data.len() < TOKEN_ACCOUNT_LEN {
        return None;
    }
    Some((Pubkey::new_from_array(data[0..32].try_into().ok()?), Pubkey::new_from_array(data[32..64].try_into().ok()?)))
}
