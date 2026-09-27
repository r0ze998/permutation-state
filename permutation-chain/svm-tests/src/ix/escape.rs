//! The escape hatches (WP14): Abort, RequestUndelegation,
//! RollbackUndelegation, CloseSeasonAccounts.

use solana_address::Address;
use solana_instruction::Instruction;

use crate::dlp_escape::{
    commit_record, commit_state, delegation_metadata, delegation_record, undelegation_request,
};
use crate::instruction::ChainInstruction as I;
use crate::season::SeasonFx;
use crate::{addr, r, rs, w, ws, DLP, SYSTEM};

impl SeasonFx {
    /// `abort`: caller (s), season, the 20 world chunks (read, any owner).
    /// Anyone, per `lifecycle::check_abort`.
    pub fn abort_ix(&self, caller: &Address) -> Instruction {
        let mut m = vec![rs(caller), w(&self.season)];
        m.extend(self.chunks.iter().map(r));
        self.ix(&I::Abort, m)
    }

    /// `requestUndelegation`: operator (s,w; the delegation's rent payer),
    /// season, target PDA, owner program, request PDA, delegation record,
    /// delegation metadata, system, DLP.
    pub fn request_undelegation_ix(&self, operator: &Address, target: u16) -> Instruction {
        let key = self.target(target);
        self.ix(
            &I::RequestUndelegation { target },
            vec![
                ws(operator),
                r(&self.season),
                r(&key),
                r(&self.program),
                w(&undelegation_request(&key)),
                r(&delegation_record(&key)),
                w(&delegation_metadata(&key)),
                r(&addr(SYSTEM)),
                r(&addr(DLP)),
            ],
        )
    }

    /// `rollbackUndelegation` (permissionless): season, target PDA, owner
    /// program, request, record, metadata, rent payer, commit state, commit
    /// record, commit reimbursement (the rent payer when no commit is
    /// pending), DLP.
    pub fn rollback_ix(&self, target: u16, rent_payer: &Address) -> Instruction {
        let key = self.target(target);
        self.ix(
            &I::RollbackUndelegation { target },
            vec![
                w(&self.season),
                w(&key),
                r(&self.program),
                w(&undelegation_request(&key)),
                w(&delegation_record(&key)),
                w(&delegation_metadata(&key)),
                w(rent_payer),
                w(&commit_state(&key)),
                w(&commit_record(&key)),
                w(rent_payer),
                r(&addr(DLP)),
            ],
        )
    }

    /// `closeSeasonAccounts`: operator (s,w), season, the targets in order
    /// (clients send at most 13).
    pub fn close_accounts_ix(&self, operator: &Address, targets: Vec<u16>) -> Instruction {
        let mut m = vec![ws(operator), r(&self.season)];
        m.extend(targets.iter().map(|t| w(&self.target(*t))));
        self.ix(&I::CloseSeasonAccounts { targets }, m)
    }
}
