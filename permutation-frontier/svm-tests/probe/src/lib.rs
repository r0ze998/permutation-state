//! The harness's stand-alone probe program (never deployable).
//!
//! Instruction data, byte 0:
//! - `0` **CreateAccount of the Season PDA**: accounts `[payer s,w] [season w]
//!   [system]`, data `0 ‖ id u64 ‖ bump u8 ‖ space u64`. The PDA signs with
//!   `["season", le64(id), bump]`.
//! - `1` **CreateAccountWithSeed of a with-seed address of the Season PDA**:
//!   accounts `[payer s,w] [target w] [season] [system]`, data `1 ‖ id u64 ‖
//!   bump u8 ‖ space u64 ‖ seed_len u8 ‖ seed`. The base (the PDA) signs.
//! - `2` **no-op** that touches the padding (loaded-data drill).
//! - `3` **CPI forwarder**: invokes `a[0]` with the remaining accounts
//!   (their signer and writable flags as passed) and `data[1..]`, so a test
//!   can show that a top-level-only instruction refuses a CPI (`NotTopLevel`).
//!
//! Deployed at the Frontier program's id on a chain of its own, (0) and (1)
//! aim at exactly the addresses the Frontier program initialises; both fail
//! on a pre-funded address (the System program refuses an account that
//! already has lamports), which is why the program never uses them (§4.2,
//! §13.2).
#![allow(unexpected_cfgs)]

use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

/// Padding length from `PSF_PROBE_PAD` at build time (0 when unset).
const PAD_LEN: usize = parse(option_env!("PSF_PROBE_PAD"));

const fn parse(s: Option<&str>) -> usize {
    let b = match s {
        Some(s) => s.as_bytes(),
        None => return 0,
    };
    let mut i = 0;
    let mut n = 0usize;
    while i < b.len() {
        n = n * 10 + (b[i] - b'0') as usize;
        i += 1;
    }
    n
}

// Not `#[used]`: that marks the section SHF_GNU_RETAIN, the linker then sets
// EI_OSABI = GNU and the loader refuses the ELF ("wrong ABI", measured on
// 3.1.9). The dynamic volatile read in tag 2 keeps every byte.
static PAD: [u8; PAD_LEN] = [0x5A; PAD_LEN];

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process);

fn u64_at(d: &[u8], o: usize) -> Result<u64, ProgramError> {
    d.get(o..o + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(ProgramError::InvalidInstructionData)
}

pub fn process(program_id: &Pubkey, a: &[AccountInfo], d: &[u8]) -> ProgramResult {
    match d.first().copied() {
        Some(0) => {
            let id = u64_at(d, 1)?;
            let bump = *d.get(9).ok_or(ProgramError::InvalidInstructionData)?;
            let space = u64_at(d, 10)?;
            let (payer, season, system) = (&a[0], &a[1], &a[2]);
            let lamports = Rent::get()?.minimum_balance(space as usize);
            let mut data = vec![0u8; 4 + 8 + 8 + 32];
            data[4..12].copy_from_slice(&lamports.to_le_bytes());
            data[12..20].copy_from_slice(&space.to_le_bytes());
            data[20..52].copy_from_slice(program_id.as_ref());
            let ix = Instruction {
                program_id: *system.key,
                accounts: vec![
                    AccountMeta::new(*payer.key, true),
                    AccountMeta::new(*season.key, true),
                ],
                data,
            };
            invoke_signed(
                &ix,
                &[payer.clone(), season.clone(), system.clone()],
                &[&[b"season", &id.to_le_bytes(), &[bump]]],
            )
        }
        Some(1) => {
            let id = u64_at(d, 1)?;
            let bump = *d.get(9).ok_or(ProgramError::InvalidInstructionData)?;
            let space = u64_at(d, 10)?;
            let n = *d.get(18).ok_or(ProgramError::InvalidInstructionData)? as usize;
            let seed = d
                .get(19..19 + n)
                .ok_or(ProgramError::InvalidInstructionData)?;
            let (payer, target, season, system) = (&a[0], &a[1], &a[2], &a[3]);
            let lamports = Rent::get()?.minimum_balance(space as usize);
            let mut data = vec![3u8, 0, 0, 0];
            data.extend_from_slice(season.key.as_ref());
            data.extend_from_slice(&(n as u64).to_le_bytes());
            data.extend_from_slice(seed);
            data.extend_from_slice(&lamports.to_le_bytes());
            data.extend_from_slice(&space.to_le_bytes());
            data.extend_from_slice(program_id.as_ref());
            let ix = Instruction {
                program_id: *system.key,
                accounts: vec![
                    AccountMeta::new(*payer.key, true),
                    AccountMeta::new(*target.key, false),
                    AccountMeta::new_readonly(*season.key, true),
                ],
                data,
            };
            invoke_signed(
                &ix,
                &[
                    payer.clone(),
                    target.clone(),
                    season.clone(),
                    system.clone(),
                ],
                &[&[b"season", &id.to_le_bytes(), &[bump]]],
            )
        }
        Some(2) => {
            #[allow(clippy::absurd_extreme_comparisons)] // PAD_LEN is 0 unless padded
            if PAD_LEN > 0 {
                let i = d.len() % PAD_LEN;
                // SAFETY: `i < PAD_LEN`; a volatile read keeps the padding in the ELF.
                let b = unsafe { core::ptr::read_volatile(&PAD[i]) };
                if b != 0x5A {
                    return Err(ProgramError::Custom(1));
                }
            }
            Ok(())
        }
        Some(3) => {
            let (target, rest) = a.split_first().ok_or(ProgramError::NotEnoughAccountKeys)?;
            let ix = Instruction {
                program_id: *target.key,
                accounts: rest
                    .iter()
                    .map(|x| AccountMeta {
                        pubkey: *x.key,
                        is_signer: x.is_signer,
                        is_writable: x.is_writable,
                    })
                    .collect(),
                data: d[1..].to_vec(),
            };
            solana_program::program::invoke(&ix, a)
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
