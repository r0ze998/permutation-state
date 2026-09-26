//! Drivers: the steps of a season as the gateway's crank sends them
//! (`permutation-gateway/src/season.mjs`, `crank.mjs`), each asserting that
//! it landed.

pub mod genesis;
pub mod tick;

pub use genesis::SEAT_BATCH;
pub use tick::TickStats;

use solana_signer::Signer;

use crate::chain::Chain;
use crate::error::ChainError;
use crate::season::SeasonFx;

/// Where a finalized season's USDC went.
#[derive(Debug, Default, Clone, Copy)]
pub struct Settled {
    /// Paid to the members' own token accounts.
    pub claims: u64,
    /// Paid to the admin (the operations share).
    pub ops: u64,
    /// Left in the vault.
    pub left: u64,
}

impl SeasonFx {
    /// After FinishSeason: every member claims to its own token account
    /// (NothingToClaim is fine) and the admin withdraws the operations share.
    pub fn settle(&self, c: &mut Chain) -> Settled {
        let mut out = Settled::default();
        for (i, m) in self.members.iter().enumerate() {
            let wallet = m.wallet.insecure_clone();
            let before = c.balance(&m.token);
            match c.send(
                vec![self.claim_ix(i, &wallet.pubkey(), &m.token)],
                &[&wallet],
            ) {
                Ok(_) => out.claims += c.balance(&m.token) - before,
                Err(f) => assert_eq!(
                    f.code,
                    Some(ChainError::NothingToClaim as u32),
                    "claim {i}: {f:#?}"
                ),
            }
        }
        let admin = self.admin.insecure_clone();
        let dest = c.token_account(&self.mint, &admin.pubkey(), 0);
        c.send(vec![self.withdraw_ix(&admin.pubkey(), &dest)], &[&admin])
            .expect("WithdrawOps");
        out.ops = c.balance(&dest);
        out.left = c.balance(&self.vault);
        out
    }
}
