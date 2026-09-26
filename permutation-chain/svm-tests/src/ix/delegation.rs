//! Delegate, Commit, CommitAndUndelegate, CommitPart, UndelegatePart (the
//! undelegate callback is `magicblock::callback_ix`).

use solana_address::Address;
use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::magicblock::{magic_context, magic_program};
use crate::season::SeasonFx;
use crate::{addr, pda, r, w, ws, DLP, SYSTEM};

impl SeasonFx {
    /// `delegate` (`chain.mjs:102`): authority (s,w), system, season, PDA,
    /// owner program, buffer, delegation record, delegation metadata, DLP.
    pub fn delegate_ix(&self, authority: &Address, target: u16) -> Instruction {
        let key = self.target(target);
        let buffer = pda(&self.program, &[b"buffer", key.as_ref()]);
        let record = pda(&addr(DLP), &[b"delegation", key.as_ref()]);
        let metadata = pda(&addr(DLP), &[b"delegation-metadata", key.as_ref()]);
        self.ix(
            &I::Delegate { target },
            vec![
                ws(authority),
                r(&addr(SYSTEM)),
                r(&self.season),
                w(&key),
                r(&self.program),
                w(&buffer),
                w(&record),
                w(&metadata),
                r(&addr(DLP)),
            ],
        )
    }

    /// `Commit` / `CommitAndUndelegate` (no client builder: local stacks
    /// only): payer (s,w), magic program, magic context, the world chunks,
    /// every nation.
    pub fn commit_ix(&self, payer: &Address, undelegate: bool) -> Instruction {
        let mut m = vec![ws(payer), r(&magic_program()), w(&magic_context())];
        m.extend(self.tick_metas());
        self.ix(
            &if undelegate {
                I::CommitAndUndelegate
            } else {
                I::Commit
            },
            m,
        )
    }

    /// `commitPart` / `undelegatePart` (`chain.mjs:143-158`): payer (s,w),
    /// magic program, magic context, world chunk 0, (CommitPart: nation 0,
    /// which records the crank), then every target but chunk 0 in order.
    pub fn part_ix(&self, payer: &Address, targets: Vec<u16>, undelegate: bool) -> Instruction {
        let mut m = vec![
            ws(payer),
            r(&magic_program()),
            w(&magic_context()),
            w(&self.chunks[0]),
        ];
        if !undelegate {
            m.push(r(&self.nations[0]));
        }
        m.extend(
            targets
                .iter()
                .filter(|t| **t != 0)
                .map(|t| w(&self.target(*t))),
        );
        self.ix(
            &if undelegate {
                I::UndelegatePart { targets }
            } else {
                I::CommitPart { targets }
            },
            m,
        )
    }
}
