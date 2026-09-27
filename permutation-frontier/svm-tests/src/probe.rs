//! The harness's stand-alone probe program (`probe/`, never deployable),
//! built by `run.sh` into `target/deploy-probe/psf_probe.so` (SBPF v2).
//!
//! - **SIMD-0186 control** (`g01_loaded_limit_control_*`): a program that
//!   does nothing, deployed at any `max_len`, so the harness's loaded-data
//!   enforcement is proven on a known account set (the numbers the W2-B
//!   validator drill measured on `solana-test-validator 3.1.9`).
//! - **Pre-funding regression** (G2, §13.2): deployed at the Frontier
//!   program's id on a chain of its own, it signs as the Season PDA and asks
//!   the System program for `CreateAccount` / `CreateAccountWithSeed` of
//!   the exact addresses the Frontier program initialises; both fail once
//!   the address holds lamports.
//! - **CPI forwarder** (G13 `NotTopLevel`): invokes a Frontier instruction
//!   from inside another program.

use std::path::PathBuf;

use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::chain::repo_root;

/// The probe binary: `PSF_PROBE_SO`, else the harness's build.
pub fn so_path() -> PathBuf {
    std::env::var_os("PSF_PROBE_SO")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root().join("permutation-frontier/svm-tests/target/deploy-probe/psf_probe.so")
        })
}

pub fn load() -> Vec<u8> {
    let p = so_path();
    std::fs::read(&p).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (build it: run.sh, or `cd permutation-frontier/svm-tests/probe && cargo-build-sbf --tools-version v1.52 --arch v2 --sbf-out-dir ../target/deploy-probe`)",
            p.display()
        )
    })
}

/// Tag 2: a no-op listing `extra` read-only accounts.
pub fn noop(program: Address, extra: &[Address]) -> Instruction {
    Instruction {
        program_id: program,
        accounts: extra
            .iter()
            .map(|k| AccountMeta::new_readonly(*k, false))
            .collect(),
        data: vec![2, 0, 0],
    }
}

/// Tag 0: CreateAccount of the Season PDA `["season", le64(id), bump]`.
pub fn create_season_pda(
    program: Address,
    payer: Address,
    id: u64,
    bump: u8,
    space: u64,
) -> Instruction {
    let (season, _) = fclient::addr::season_pda(&program, id);
    let mut data = vec![0u8];
    data.extend_from_slice(&id.to_le_bytes());
    data.push(bump);
    data.extend_from_slice(&space.to_le_bytes());
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(season, false),
            AccountMeta::new_readonly(Address::default(), false),
        ],
        data,
    }
}

/// Tag 1: CreateAccountWithSeed of `target = with_seed(season_pda, seed, program)`.
pub fn create_with_seed(
    program: Address,
    payer: Address,
    id: u64,
    bump: u8,
    seed: &[u8],
    space: u64,
) -> Instruction {
    let (season, _) = fclient::addr::season_pda(&program, id);
    let target = fclient::addr::with_seed(&season, seed, &program);
    let mut data = vec![1u8];
    data.extend_from_slice(&id.to_le_bytes());
    data.push(bump);
    data.extend_from_slice(&space.to_le_bytes());
    data.push(seed.len() as u8);
    data.extend_from_slice(seed);
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(target, false),
            AccountMeta::new_readonly(season, false),
            AccountMeta::new_readonly(Address::default(), false),
        ],
        data,
    }
}

/// Tag 3: the probe at `probe` invokes `inner` by CPI (for `NotTopLevel`).
/// The inner instruction's accounts follow its program id, flags kept.
pub fn cpi(probe: Address, inner: &Instruction) -> Instruction {
    let mut accounts = vec![AccountMeta::new_readonly(inner.program_id, false)];
    accounts.extend(inner.accounts.iter().cloned());
    let mut data = vec![3u8];
    data.extend_from_slice(&inner.data);
    Instruction {
        program_id: probe,
        accounts,
        data,
    }
}
