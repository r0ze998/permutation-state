//! The two SPL Token instructions the program needs, built by hand so the
//! program carries no token-crate version pins (SPL Token v3 layout).

use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    pubkey::Pubkey,
};

pub const TOKEN_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
/// Size of an SPL token account.
pub const TOKEN_ACCOUNT_LEN: usize = 165;

/// `InitializeAccount3 { owner }` (tag 18).
pub fn initialize_account3<'a>(
    account: &AccountInfo<'a>,
    mint: &AccountInfo<'a>,
    owner: &Pubkey,
) -> ProgramResult {
    let mut data = Vec::with_capacity(33);
    data.push(18);
    data.extend_from_slice(owner.as_ref());
    let ix = Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*account.key, false),
            AccountMeta::new_readonly(*mint.key, false),
        ],
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
    let infos = [
        source.clone(),
        mint.clone(),
        destination.clone(),
        authority.clone(),
    ];
    match seeds {
        Some(s) => invoke_signed(&ix, &infos, &[s]),
        None => invoke(&ix, &infos),
    }
}

/// Size of an SPL mint.
pub const MINT_LEN: usize = 82;

/// Circle's USDC mint on mainnet: the only mint the `mainnet` build
/// accepts. Its freeze authority is a documented trust root.
pub const USDC_MAINNET: Pubkey =
    solana_program::pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// An initialized SPL mint's data (Token-owned, `MINT_LEN` bytes,
/// `is_initialized` at byte 45).
fn mint_data(mint: &AccountInfo) -> Option<[u8; MINT_LEN]> {
    if mint.owner != &TOKEN_PROGRAM_ID {
        return None;
    }
    let data: [u8; MINT_LEN] = mint.try_borrow_data().ok()?[..].try_into().ok()?;
    (data[45] == 1).then_some(data)
}

/// Mint decimals (byte 44) of an initialized mint.
pub fn mint_decimals(mint: &AccountInfo) -> Option<u8> {
    mint_data(mint).map(|d| d[44])
}

/// Whether an initialized mint has a freeze authority (its
/// `COption<Pubkey>` tag at 46..50; the key is at 50..82). A freeze
/// authority can freeze the vault, and every Claim then fails.
pub fn mint_has_freeze_authority(mint: &AccountInfo) -> Option<bool> {
    mint_data(mint).map(|d| d[46..50] != [0; 4])
}

/// Whether `mint` is the mint this build pins: Circle USDC in `mainnet`
/// builds, none otherwise.
pub fn is_pinned_usdc(mint: &Pubkey) -> bool {
    cfg!(feature = "mainnet") && *mint == USDC_MAINNET
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
    Some((
        Pubkey::new_from_array(data[0..32].try_into().ok()?),
        Pubkey::new_from_array(data[32..64].try_into().ok()?),
    ))
}

/// The amount (u64 at 64..72) of an SPL token account.
pub fn token_account_amount(account: &AccountInfo) -> Option<u64> {
    if account.owner != &TOKEN_PROGRAM_ID {
        return None;
    }
    let data = account.try_borrow_data().ok()?;
    if data.len() < TOKEN_ACCOUNT_LEN {
        return None;
    }
    Some(u64::from_le_bytes(data[64..72].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An SPL mint: `decimals`, initialized or not, with or without a
    /// freeze authority.
    fn mint(decimals: u8, initialized: bool, freeze: bool) -> Vec<u8> {
        let mut d = vec![0u8; MINT_LEN];
        d[0..4].copy_from_slice(&1u32.to_le_bytes()); // mint authority: Some
        d[44] = decimals;
        d[45] = initialized as u8;
        if freeze {
            d[46..50].copy_from_slice(&1u32.to_le_bytes());
            d[50..82].copy_from_slice(&[9; 32]);
        }
        d
    }

    fn check<R>(owner: &Pubkey, mut data: Vec<u8>, f: impl FnOnce(&AccountInfo) -> R) -> R {
        let key = Pubkey::new_unique();
        let mut lamports = 0u64;
        let info = AccountInfo::new(&key, false, false, &mut lamports, &mut data, owner, false);
        f(&info)
    }

    #[test]
    fn mints_are_read_only_when_initialized_and_well_formed() {
        let t = TOKEN_PROGRAM_ID;
        assert_eq!(check(&t, mint(6, true, false), mint_decimals), Some(6));
        assert_eq!(
            check(&t, mint(6, true, false), mint_has_freeze_authority),
            Some(false)
        );
        assert_eq!(
            check(&t, mint(9, true, true), mint_has_freeze_authority),
            Some(true)
        );
        // Uninitialized, wrong length or wrong owner: not a mint.
        assert_eq!(check(&t, mint(6, false, false), mint_decimals), None);
        let mut long = mint(6, true, false);
        long.push(0);
        assert_eq!(check(&t, long, mint_decimals), None);
        assert_eq!(check(&t, vec![6u8; 45], mint_decimals), None);
        let other = Pubkey::new_unique();
        assert_eq!(
            check(&other, mint(6, true, false), mint_has_freeze_authority),
            None
        );
    }

    #[test]
    fn token_accounts_give_mint_owner_and_amount() {
        let (m, o) = (Pubkey::new_unique(), Pubkey::new_unique());
        let mut d = vec![0u8; TOKEN_ACCOUNT_LEN];
        d[0..32].copy_from_slice(m.as_ref());
        d[32..64].copy_from_slice(o.as_ref());
        d[64..72].copy_from_slice(&1_234_567u64.to_le_bytes());
        let t = TOKEN_PROGRAM_ID;
        assert_eq!(check(&t, d.clone(), token_account_amount), Some(1_234_567));
        assert_eq!(check(&t, d.clone(), token_account_mint_owner), Some((m, o)));
        assert_eq!(check(&t, d[..100].to_vec(), token_account_amount), None);
        assert_eq!(check(&Pubkey::new_unique(), d, token_account_amount), None);
    }

    #[test]
    fn only_mainnet_builds_pin_usdc() {
        assert_eq!(is_pinned_usdc(&USDC_MAINNET), cfg!(feature = "mainnet"));
        assert!(!is_pinned_usdc(&Pubkey::new_unique()));
    }
}
