//! Delegate, Commit, CommitAndUndelegate, CommitPart, UndelegatePart (the
//! undelegate callback is `magicblock::callback_ix`).

use solana_address::Address;
use solana_instruction::Instruction;

use crate::instruction::ChainInstruction as I;
use crate::ix::registration::ER_VALIDATOR;
use crate::magicblock::{magic_context, magic_program};
use crate::season::SeasonFx;
use crate::{addr, pda, r, w, ws, DLP, SYSTEM};

/// The Instructions sysvar (`UndelegatePart` checks it is alone).
pub const SYSVAR_INSTRUCTIONS: &str = "Sysvar1nstructions1111111111111111111111111";

impl SeasonFx {
    /// `delegate` (`chain.mjs`): authority (s,w), system, season (w), PDA,
    /// owner program, buffer, delegation record, delegation metadata, DLP,
    /// the validator the season was created with (`ER_VALIDATOR`).
    pub fn delegate_ix(&self, authority: &Address, target: u16) -> Instruction {
        self.delegate_to_ix(authority, target, &addr(ER_VALIDATOR))
    }

    /// `delegate_ix` naming `validator`.
    pub fn delegate_to_ix(
        &self,
        authority: &Address,
        target: u16,
        validator: &Address,
    ) -> Instruction {
        let key = self.target(target);
        let buffer = pda(&self.program, &[b"buffer", key.as_ref()]);
        let record = pda(&addr(DLP), &[b"delegation", key.as_ref()]);
        let metadata = pda(&addr(DLP), &[b"delegation-metadata", key.as_ref()]);
        self.ix(
            &I::Delegate { target },
            vec![
                ws(authority),
                r(&addr(SYSTEM)),
                w(&self.season),
                w(&key),
                r(&self.program),
                w(&buffer),
                w(&record),
                w(&metadata),
                r(&addr(DLP)),
                r(validator),
            ],
        )
    }

    /// The retired `Commit` / `CommitAndUndelegate` (no client builder):
    /// payer (s,w), magic program, magic context, the world chunks, every
    /// nation. Always `Retired`.
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

    /// `commitPart` / `undelegatePart` (`chain.mjs`): payer (s,w), magic
    /// program, magic context, world chunk 0, (CommitPart: nation 0, which
    /// records the crank), then every target but chunk 0 in order
    /// (UndelegatePart: then the Instructions sysvar).
    pub fn part_ix(&self, payer: &Address, targets: Vec<u16>, undelegate: bool) -> Instruction {
        if undelegate {
            return self.undelegate_ix(payer, targets, &[]);
        }
        let mut m = vec![
            ws(payer),
            r(&magic_program()),
            w(&magic_context()),
            w(&self.chunks[0]),
            r(&self.nations[0]),
        ];
        m.extend(
            targets
                .iter()
                .filter(|t| **t != 0)
                .map(|t| w(&self.target(*t))),
        );
        self.ix(&I::CommitPart { targets }, m)
    }

    /// `undelegatePart({ gone })`: the targets in `gone` (already left the
    /// ER) are passed read-only; the Instructions sysvar last.
    pub fn undelegate_ix(&self, payer: &Address, targets: Vec<u16>, gone: &[u16]) -> Instruction {
        let mut m = vec![
            ws(payer),
            r(&magic_program()),
            w(&magic_context()),
            w(&self.chunks[0]),
        ];
        m.extend(targets.iter().filter(|t| **t != 0).map(|t| {
            if gone.contains(t) {
                r(&self.target(*t))
            } else {
                w(&self.target(*t))
            }
        }));
        m.push(r(&addr(SYSVAR_INSTRUCTIONS)));
        self.ix(&I::UndelegatePart { targets }, m)
    }
}
