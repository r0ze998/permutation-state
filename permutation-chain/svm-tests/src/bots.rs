//! `BotSeason`: the play server's bots (`permutation_server::driver`) play a
//! season through the SBF build — sealed orders, governance, every tick —
//! and every tick is checked against the native rules on the published input.
//!
//! The bots use `Ledger::seeded`, so two runs of the same tree are identical
//! to the CU (`Ledger::new` mixes the clock and the process id in): budget
//! numbers reproduce and a CI failure can be replayed.

use borsh::BorshDeserialize;
use permutation_rules::gov::{Role, NOBODY};
use permutation_rules::orders::{order_commitment, OrderBatch};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::Ruleset;
use permutation_server::driver::{local_key, AllAi, Planner};
use permutation_server::{fog::Fog, ledger::Ledger};
use solana_address::Address;
use solana_instruction::Instruction;
use solana_signer::Signer;

use crate::budget::{resolve_parts, Need, Part};
use crate::chain::Chain;
use crate::season::{Params, SeasonFx};

/// What one bot tick did and cost.
#[derive(Debug, Default)]
pub struct TickReport {
    /// The tick that was resolved.
    pub tick: u16,
    /// Worst CU per player instruction kind this tick.
    pub submit_gov: u64,
    pub commit: u64,
    pub reveal: u64,
    /// Worst CU of the crank's CloseCommits and LogTickInput (the ResolveTick
    /// parts are in `parts`).
    pub log: u64,
    pub parts: Vec<Part>,
    /// Submissions the program refused (none expected from honest bots).
    pub refused: usize,
    pub batches: usize,
    pub gov: usize,
    /// With `measure`: the bisected needs of the largest RevealOrders and
    /// SubmitGov of the tick (by instruction size).
    pub reveal_need: Option<Need>,
    pub gov_need: Option<Need>,
}

pub struct BotSeason {
    pub s: SeasonFx,
    pub state: WorldState,
    pub rules: Ruleset,
    planner: Planner,
    fog: Fog,
    ledger: Ledger,
}

/// The salt a bot seals a batch with (deterministic).
fn salt(b: &OrderBatch) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[0] = b.civ as u8;
    x[1] = b.role as u8;
    x[2..4].copy_from_slice(&b.tick.to_le_bytes());
    x[31] = 7;
    x
}

impl BotSeason {
    /// A Blitz season of `per_nation[civ]` bot members per nation, seated
    /// and open, on a chain with sigverify off (`Chain::new_opts(false)`:
    /// the bots' session keys are `local_key` hashes without private keys).
    /// Members register in nation order, standing for two rotating offices
    /// and voting for themselves, as `seat_ai_members` seats them.
    pub fn new(c: &mut Chain, p: Params, per_nation: &[usize]) -> BotSeason {
        assert!(
            !c.sigverify,
            "bots sign with send_as: Chain::new_opts(false)"
        );
        assert_eq!(per_nation.len(), p.nations as usize);
        let mut s = SeasonFx::create(c, p);
        let mut id = 0u32;
        for (civ, k) in per_nation.iter().enumerate() {
            for j in 0..*k {
                let roles = [Role::ALL[(2 * j) % 4], Role::ALL[(2 * j + 1) % 4]];
                let mut votes = [NOBODY; 4];
                for r in roles {
                    votes[r.index()] = id;
                }
                s.register_raw(
                    c,
                    civ as u16,
                    local_key(id),
                    roles[0].bit() | roles[1].bit(),
                    votes,
                    0,
                    [0; 32],
                );
                id += 1;
            }
        }
        s.genesis(c);
        s.seat_and_open(c);
        let state = s.world(c);
        BotSeason {
            rules: s.rules(),
            planner: Planner::new(per_nation.len()),
            fog: Fog::new(&state),
            ledger: Ledger::seeded(b"svm-tests"),
            state,
            s,
        }
    }

    /// Whether the last tick resolved.
    pub fn done(&self) -> bool {
        self.state.tick >= self.rules.ticks_per_season
    }

    /// One tick: the bots plan (governance and batches); SubmitGov and
    /// CommitOrders are sent with their session keys, CloseCommits, the
    /// reveals, LogTickInput, then ResolveTick in the crank's parts. The
    /// tick is resolved natively on the published input and the on-chain
    /// root must equal the native one. With `measure`, every crank part and
    /// the largest RevealOrders and SubmitGov are bisected (`need`).
    pub fn step(&mut self, c: &mut Chain, measure: bool) -> TickReport {
        let s = &self.s;
        let crank = s.crank.insecure_clone();
        let cp = crank.pubkey();
        let mut rep = TickReport {
            tick: self.state.tick,
            ..Default::default()
        };
        self.ledger.observe(&self.state, &self.fog);
        let gov = self
            .planner
            .member_gov(&self.state, &self.rules, &self.fog, &AllAi);
        let batches = self.planner.batches(
            &self.state,
            &self.rules,
            &self.fog,
            &mut self.ledger,
            &AllAi,
        );
        let gov_ixs: Vec<Instruction> = gov
            .iter()
            .map(|e| {
                let civ = self.state.members[e.member as usize].civ;
                s.submit_gov_ix(
                    &Address::new_from_array(e.signer),
                    civ,
                    e.member,
                    e.action.clone(),
                )
            })
            .collect();
        let big_gov = (0..gov_ixs.len()).max_by_key(|i| gov_ixs[*i].data.len());
        for (i, ix) in gov_ixs.into_iter().enumerate() {
            if measure && Some(i) == big_gov {
                rep.gov_need = Some(
                    c.need_as(std::slice::from_ref(&ix), &cp)
                        .expect("SubmitGov lands"),
                );
            }
            match c.send_as(vec![ix], &cp) {
                Ok(l) => {
                    rep.gov += 1;
                    rep.submit_gov = rep.submit_gov.max(l.cu);
                }
                Err(_) => rep.refused += 1,
            }
        }
        for b in &batches {
            let signer = Address::new_from_array(local_key(b.member));
            let ix = s.commit_orders_ix(
                &signer,
                b.civ,
                b.role,
                b.tick,
                order_commitment(b, &salt(b)),
            );
            let l = c.send_as(vec![ix], &cp).expect("CommitOrders");
            rep.commit = rep.commit.max(l.cu);
        }
        rep.log = s.close(c);
        let big_batch =
            (0..batches.len()).max_by_key(|i| borsh::to_vec(&batches[*i]).unwrap().len());
        for (i, b) in batches.iter().enumerate() {
            let ix = s.reveal_orders_ix(&cp, b, salt(b));
            if measure && Some(i) == big_batch {
                rep.reveal_need = Some(
                    c.need_as(std::slice::from_ref(&ix), &cp)
                        .expect("RevealOrders lands"),
                );
            }
            match c.send_as(vec![ix], &cp) {
                Ok(l) => {
                    rep.batches += 1;
                    rep.reveal = rep.reveal.max(l.cu);
                }
                Err(f) => {
                    rep.refused += 1;
                    println!("tick {} reveal refused: {}", b.tick, f.err);
                }
            }
        }
        let (input, log) = s.log_input(c);
        rep.log = rep.log.max(log);
        let input = TickInput::try_from_slice(&input).unwrap();
        rep.parts = resolve_parts(c, s, &crank, measure);
        let native = resolve_tick(&mut self.state, &self.rules, &input).unwrap();
        assert_eq!(s.root(c), native, "tick {}: chain equals native", rep.tick);
        self.fog.update(&self.state);
        rep
    }
}
