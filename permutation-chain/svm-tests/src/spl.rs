//! Hand-packed SPL Token v3 accounts (LiteSVM ships the token program; the
//! repository pins no spl-token crate): Mint 82 B, Account 165 B.

use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::{addr, Chain, TOKEN};

pub const MINT_LEN: usize = 82;
pub const TOKEN_ACCOUNT_LEN: usize = 165;

impl Chain {
    /// A new initialized mint (mint authority set, no freeze authority).
    pub fn mint(&mut self, decimals: u8) -> Address {
        let key = Keypair::new().pubkey();
        let mut d = vec![0u8; MINT_LEN];
        d[0..4].copy_from_slice(&1u32.to_le_bytes()); // mint authority: Some
        d[4..36].copy_from_slice(&[9; 32]);
        d[44] = decimals;
        d[45] = 1; // initialized
        self.put(key, addr(TOKEN), d);
        key
    }

    /// A new initialized token account of `mint` owned by `owner`.
    pub fn token_account(&mut self, mint: &Address, owner: &Address, amount: u64) -> Address {
        let key = Keypair::new().pubkey();
        let mut d = vec![0u8; TOKEN_ACCOUNT_LEN];
        d[0..32].copy_from_slice(mint.as_ref());
        d[32..64].copy_from_slice(owner.as_ref());
        d[64..72].copy_from_slice(&amount.to_le_bytes());
        d[108] = 1; // AccountState::Initialized
        self.put(key, addr(TOKEN), d);
        key
    }

    pub fn balance(&self, token_account: &Address) -> u64 {
        let d = self.data(token_account);
        u64::from_le_bytes(d[64..72].try_into().unwrap())
    }

    /// Overwrites a token account's amount (e.g. an underfunded vault).
    pub fn set_balance(&mut self, token_account: &Address, amount: u64) {
        let mut d = self.data(token_account);
        d[64..72].copy_from_slice(&amount.to_le_bytes());
        self.set_data(token_account, d);
    }

    /// (mint, owner) of a token account.
    pub fn token_mint_owner(&self, token_account: &Address) -> (Address, Address) {
        let d = self.data(token_account);
        (
            Address::try_from(&d[0..32]).unwrap(),
            Address::try_from(&d[32..64]).unwrap(),
        )
    }
}
