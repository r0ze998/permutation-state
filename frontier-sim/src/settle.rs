//! The Reckoning: Shades revealed and voided, faction indices folded, the
//! season settled and every claim paid through the payout kernel, then
//! conservation checked. The same finished season can be settled under
//! several index parameters (γ), since γ changes only the settlement.

use crate::model::{Arch, BUILDER_THRESHOLD};
use crate::sim::{Earned, JoinState, Sim, NONE};
use permutation_rules::frontier::index::{
    clamped_mean, faction_index, herding, FactionFacts, IndexParams, FACTIONS,
};
use permutation_rules::frontier::payout::{
    claim, settle, tenure_units, CitizenRecord, Claim, FactionTotals, Ledger, PayoutParams,
    Settlement,
};
use permutation_rules::frontier::pools::{steward_rows, Pools};
use permutation_rules::hash::sha256;

#[derive(Clone, Debug)]
pub struct AgentOut {
    pub arch: Arch,
    pub shade: bool,
    pub faction: u8,
    pub join_day: u32,
    pub fee: u64,
    pub stake: u64,
    pub claim: Claim,
    pub laurels: u64,
    pub works: u64,
    pub builder: bool,
    pub holdings: usize,
    /// Tier of the first holding at T_end (0 Hamlet … 3 Stronghold; 4 = none).
    pub first_tier: u8,
    /// First holding's province at T_end: attached holdings, first
    /// holdings among them, own weight / province mean weight.
    pub nbhd: (u32, u32, f64),
    pub earned: Earned,
    pub sessions: u32,
}

impl AgentOut {
    pub fn paid(&self) -> u64 {
        self.fee + self.stake
    }
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub facts: [FactionFacts; FACTIONS],
    /// Undamped per-capita index `clamp(mean ratio)`.
    pub raw: [u64; FACTIONS],
    /// Herding damping `h_k`.
    pub herd: [u64; FACTIONS],
    /// `s_k`.
    pub index: [u64; FACTIONS],
    pub settlement: Settlement,
    pub pools: Pools,
    pub agents: Vec<AgentOut>,
    pub ledger: Ledger,
    pub checks: Vec<Check>,
    /// sha256 over every claim and laurel balance (determinism).
    pub digest: [u8; 32],
}

fn check(v: &mut Vec<Check>, name: &'static str, ok: bool, detail: String) {
    v.push(Check { name, ok, detail });
}

pub fn settle_run(sim: &Sim, p: &IndexParams) -> Outcome {
    let sched = sim.sched;
    // Records of every citizen with a settled holding (withdrawn joins
    // were refunded and are not citizens).
    let mut recs: Vec<(usize, CitizenRecord)> = Vec::new();
    for (i, a) in sim.agents.iter().enumerate() {
        if a.state != JoinState::Settled {
            continue;
        }
        recs.push((
            i,
            CitizenRecord {
                faction: a.faction,
                fee: a.fee,
                stake: a.stake,
                tenure: tenure_units(a.active_days, sched.days_available(a.join_day)),
                works: a.works,
                builder: a.pledged >= BUILDER_THRESHOLD,
                laurels: a.laurels,
                laurels_at_stake: a.laurels_at_stake,
                steward: steward_rows(a.minister_terms, a.warden_terms),
                voided: false,
            },
        ));
    }
    // FactionShards: every credit added; RevealShade removes each Shade.
    let mut totals = [FactionTotals::default(); FACTIONS];
    for (_, r) in &recs {
        totals[r.faction as usize]
            .add(&r.weights())
            .expect("totals");
    }
    for (i, r) in recs.iter_mut() {
        if sim.agents[*i].shade {
            totals[r.faction as usize]
                .remove(&r.weights())
                .expect("void");
            r.voided = true;
        }
    }
    let facts = sim.faction_facts(true, sim.cfg.days);
    let index = faction_index(&facts, p);
    let members_all: u64 = facts.iter().map(|f| f.members).sum();
    let raw: [u64; FACTIONS] = core::array::from_fn(|k| clamped_mean(k, &facts, p));
    let herd: [u64; FACTIONS] = core::array::from_fn(|k| herding(facts[k].members, members_all, p));
    let st = settle(
        &sim.pools,
        &totals,
        &index,
        sim.engine_stages,
        &PayoutParams::REV2,
    )
    .expect("settle");
    let prize = sim.pools.prize().expect("prize");
    let mut ledger = Ledger::new(prize);
    let mut agents = Vec::with_capacity(recs.len());
    let mut checks = Vec::new();
    let mut over_cap = 0u64;
    let mut voided_paid = 0u64;
    let mut sum_paid = 0u128;
    let mut hash_in: Vec<u8> = Vec::with_capacity(recs.len() * 24);
    for (i, r) in &recs {
        let c = claim(&st, r).expect("claim");
        ledger.record(&c).expect("ledger");
        let a = &sim.agents[*i];
        if c.paid > 5 * r.paid() {
            over_cap += 1;
        }
        if r.voided && c.paid > 0 {
            voided_paid += 1;
        }
        sum_paid += c.paid as u128;
        hash_in.extend_from_slice(&c.paid.to_le_bytes());
        hash_in.extend_from_slice(&a.laurels.to_le_bytes());
        hash_in.extend_from_slice(&a.works.to_le_bytes());
        agents.push(AgentOut {
            arch: a.arch,
            shade: a.shade,
            faction: a.faction,
            join_day: a.join_day,
            fee: a.fee,
            stake: a.stake,
            claim: c,
            laurels: a.laurels,
            works: a.works,
            builder: r.builder,
            holdings: a.holdings.len(),
            first_tier: a
                .holdings
                .first()
                .map_or(4, |&h| sim.holds[h as usize].h.tier as u8),
            nbhd: a
                .holdings
                .first()
                .map_or((0, 0, f64::NAN), |&h| sim.neighbourhood(h)),
            earned: a.earned,
            sessions: a.sessions,
        });
    }
    // ---- money conservation (design §5.4 F1)
    let total = sim.pools.total().expect("total");
    check(
        &mut checks,
        "vault = paid - withdrawn",
        total as u128 == sim.paid_in as u128 - sim.withdrawn as u128,
        format!(
            "pools.total {} = paid {} - withdrawn {}",
            total, sim.paid_in, sim.withdrawn
        ),
    );
    check(
        &mut checks,
        "no escrow left pending",
        sim.pools.pending == 0,
        format!("pending {}", sim.pools.pending),
    );
    let op = sim.pools.operator_citizen + sim.pools.operator_laurel;
    let paid_settled = total - sim.pools.pending;
    check(
        &mut checks,
        "operator escrow = 20%",
        op == paid_settled - prize
            && (op as u128 * 5).abs_diff(paid_settled as u128) <= recs.len() as u128 * 10,
        format!("operator {} of settled {}", op, paid_settled),
    );
    check(
        &mut checks,
        "claims + swept + dust = prize",
        ledger.claimed as u128 + ledger.swept as u128 + ledger.dust() as u128 == prize as u128
            && sum_paid == ledger.claimed as u128,
        format!(
            "claimed {} + swept {} + dust {} = prize {}",
            ledger.claimed,
            ledger.swept,
            ledger.dust(),
            prize
        ),
    );
    check(
        &mut checks,
        "allocated <= prize",
        st.allocated <= prize,
        format!("allocated {} prize {}", st.allocated, prize),
    );
    check(
        &mut checks,
        "wallet cap 5x paid",
        over_cap == 0,
        format!("{over_cap} claims above 5x"),
    );
    check(
        &mut checks,
        "voided Shades paid nothing",
        voided_paid == 0,
        format!("{voided_paid} voided claims > 0"),
    );
    // ---- laurel conservation (zero-sum emission, §5.4)
    let (emitted, orphaned) = sim.laurels_minted();
    let held: u128 = sim.agents.iter().map(|a| a.laurels as u128).sum::<u128>()
        + sim.reserve.iter().map(|&x| x as u128).sum::<u128>()
        + sim.escrow as u128
        + sim.stats.siege_stake_orphaned as u128
        + sim.stats.laurels_burned as u128;
    let sources = sim.laurel_credited - sim.laurel_orphan_extra + sim.stats.relic_minted as u128;
    check(
        &mut checks,
        "laurels: held = credited + relic minted",
        held == sources,
        format!("held {held} sources {sources}"),
    );
    let idx_dust = emitted as i128 - sim.laurel_credited as i128 - orphaned as i128;
    check(
        &mut checks,
        "reward index: credited <= emitted - orphaned",
        idx_dust >= 0,
        format!(
            "emitted {emitted}, credited {}, orphaned {orphaned}, rounding dust {idx_dust} base units",
            sim.laurel_credited
        ),
    );
    let _ = NONE;
    Outcome {
        facts,
        raw,
        herd,
        index,
        settlement: st,
        pools: sim.pools,
        agents,
        ledger,
        checks,
        digest: sha256(&[b"sim/digest", &hash_in]),
    }
}
