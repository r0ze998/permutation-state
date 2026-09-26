//! CreateSeason, AllocWorld, AllocNation, Register, UpdateMember.

use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_signer::Signer;

use crate::instruction::ChainInstruction as I;
use crate::season::{MemberFx, SeasonFx};
use crate::{addr, r, rs, w, ws, SYSTEM, TOKEN};

/// `CreateSeason`'s data, field by field.
#[derive(Clone, Debug)]
pub struct CreateArgs {
    pub season_id: u64,
    pub preset: u8,
    pub nations: u8,
    pub entry_fee: u64,
    pub tick_seconds: u32,
    pub world_seed: [u8; 32],
    pub crank: [u8; 32],
    pub market: bool,
    pub prev_season_id: u64,
    pub ai_count: u16,
    pub roster_chain: [u8; 32],
    pub bounty_each: u64,
    pub bond: u64,
}

impl CreateArgs {
    pub fn data(&self) -> I {
        I::CreateSeason {
            season_id: self.season_id,
            preset: self.preset,
            nations: self.nations,
            entry_fee: self.entry_fee,
            tick_seconds: self.tick_seconds,
            world_seed: self.world_seed,
            crank: self.crank,
            market: self.market,
            prev_season_id: self.prev_season_id,
            ai_count: self.ai_count,
            roster_chain: self.roster_chain,
            bounty_each: self.bounty_each,
            bond: self.bond,
        }
    }
}

/// `Register`'s data, field by field.
#[derive(Clone, Debug)]
pub struct RegisterArgs {
    pub civ: u16,
    pub name: String,
    pub kind: u8,
    pub session: [u8; 32],
    pub attestation: [u8; 32],
    pub stand: u8,
    pub votes: [u32; 4],
    pub deposit: u64,
    pub tag: [u8; 32],
}

impl RegisterArgs {
    pub fn data(&self) -> I {
        I::Register {
            civ: self.civ,
            name: self.name.clone(),
            kind: self.kind,
            session: self.session,
            attestation: self.attestation,
            stand: self.stand,
            votes: self.votes,
            deposit: self.deposit,
            tag: self.tag,
        }
    }
}

impl SeasonFx {
    /// This season's `CreateSeason` data (no AIs, no predecessor).
    pub fn create_args(&self) -> CreateArgs {
        CreateArgs {
            season_id: self.p.id,
            preset: self.p.preset,
            nations: self.p.nations,
            entry_fee: self.p.fee,
            tick_seconds: self.p.tick_seconds,
            world_seed: [1; 32],
            crank: self.crank.pubkey().to_bytes(),
            market: self.p.market,
            prev_season_id: 0,
            ai_count: 0,
            roster_chain: [0; 32],
            bounty_each: 0,
            bond: 0,
        }
    }

    /// `createSeason` (`chain.mjs:63`): admin, season, vault, mint, token
    /// program, system, then `extra` (the previous season; with AIs the
    /// admin's USDC account and the roster).
    pub fn create_ix_from(
        &self,
        admin: &Address,
        a: &CreateArgs,
        extra: Vec<AccountMeta>,
    ) -> Instruction {
        let mut m = vec![
            ws(admin),
            w(&self.season),
            w(&self.vault),
            r(&self.mint),
            r(&addr(TOKEN)),
            r(&addr(SYSTEM)),
        ];
        m.extend(extra);
        self.ix(&a.data(), m)
    }

    pub fn create_ix(&self, admin: &Address) -> Instruction {
        self.create_ix_from(admin, &self.create_args(), vec![])
    }

    /// CreateSeason with this season's AIs, bounty and bond (`chain.mjs:63`).
    pub fn create_ai_ix(&self, admin: &Address, roster_chain: [u8; 32]) -> Instruction {
        let a = CreateArgs {
            ai_count: self.ai.len() as u16,
            roster_chain,
            bounty_each: self.bounty,
            bond: self.bond,
            ..self.create_args()
        };
        self.create_ix_from(admin, &a, vec![w(&self.admin_token), w(&self.roster)])
    }

    /// `allocWorld` (`chain.mjs:71`).
    pub fn alloc_world_ix(&self, payer: &Address, chunk: u8) -> Instruction {
        let key = self.chunks.get(chunk as usize).copied().unwrap_or_else(|| {
            crate::pda(
                &self.program,
                &[crate::state::WORLD_SEED, &self.p.id.to_le_bytes(), &[chunk]],
            )
        });
        self.ix(
            &I::AllocWorld { chunk },
            vec![ws(payer), r(&self.season), w(&key), r(&addr(SYSTEM))],
        )
    }

    /// `allocNation` (`chain.mjs:74`).
    pub fn alloc_nation_ix(&self, payer: &Address, civ: u16) -> Instruction {
        let key = crate::pda(
            &self.program,
            &[
                crate::state::NATION_SEED,
                &self.p.id.to_le_bytes(),
                &civ.to_le_bytes(),
            ],
        );
        self.ix(
            &I::AllocNation { civ },
            vec![ws(payer), r(&self.season), w(&key), r(&addr(SYSTEM))],
        )
    }

    /// Registration data for `m`: named "m", human, stands for every office,
    /// no first-election votes.
    pub fn register_args(&self, m: &MemberFx, deposit: u64) -> RegisterArgs {
        RegisterArgs {
            civ: m.civ,
            name: "m".into(),
            kind: 0,
            session: m.session.pubkey().to_bytes(),
            attestation: [0; 32],
            stand: 0x0f,
            votes: [u32::MAX; 4],
            deposit,
            tag: [0; 32],
        }
    }

    /// `register` (`chain.mjs:82`): wallet (s), fee payer (s,w), season,
    /// member PDA, wallet USDC, vault, mint, token program, system.
    pub fn register_ix_from(&self, m: &MemberFx, payer: &Address, a: &RegisterArgs) -> Instruction {
        self.ix(
            &a.data(),
            vec![
                rs(&m.wallet.pubkey()),
                ws(payer),
                w(&self.season),
                w(&m.member),
                w(&m.token),
                w(&self.vault),
                r(&self.mint),
                r(&addr(TOKEN)),
                r(&addr(SYSTEM)),
            ],
        )
    }

    pub fn register_ix(&self, m: &MemberFx, payer: &Address, deposit: u64) -> Instruction {
        self.register_ix_from(m, payer, &self.register_args(m, deposit))
    }

    pub fn register_ix_with(
        &self,
        m: &MemberFx,
        payer: &Address,
        session: [u8; 32],
        stand: u8,
        votes: [u32; 4],
        deposit: u64,
        tag: [u8; 32],
    ) -> Instruction {
        let a = RegisterArgs {
            session,
            stand,
            votes,
            tag,
            ..self.register_args(m, deposit)
        };
        self.register_ix_from(m, payer, &a)
    }

    /// `updateMember` (`chain.mjs:86`): signer (wallet or session), season,
    /// member PDA of member `i`.
    pub fn update_member_ix(
        &self,
        signer: &Address,
        i: usize,
        stand: u8,
        votes: [u32; 4],
    ) -> Instruction {
        self.ix(
            &I::UpdateMember { stand, votes },
            vec![rs(signer), r(&self.season), w(&self.members[i].member)],
        )
    }
}
