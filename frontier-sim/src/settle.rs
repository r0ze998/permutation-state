//! The Reckoning: Shades revealed and voided, faction indices folded, the
//! season settled and every claim paid through the payout kernel, then
//! conservation checked. The same finished season can be settled under
//! several index parameters (γ), since γ changes only the settlement.

use crate::config::OfficePay;
use crate::model::{Arch, BUILDER_THRESHOLD};
use crate::sim::{Earned, JoinState, Sim, NONE};
use permutation_rules::frontier::index::{
    clamped_mean, faction_index, herding, FactionFacts, IndexParams, FACTIONS,
};
use permutation_rules::frontier::payout::{
    claim, settle, tenure_units, CitizenRecord, Claim, FactionTotals, PayoutParams, SeasonLedger,
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
    pub ledger: SeasonLedger,
    /// CL-07/CL-08: the money and laurel books the bound checks read.
    pub books: Books,
    pub checks: Vec<Check>,
    /// sha256 over every claim and laurel balance (determinism).
    pub digest: [u8; 32],
}

fn check(v: &mut Vec<Check>, name: &'static str, ok: bool, detail: String) {
    v.push(Check { name, ok, detail });
}

/// One claimant as the books see it (CL-07): what the rules entitle the
/// wallet to (`payout::claim` recomputed from its record) and what the
/// settlement actually paid and swept.
#[derive(Clone, Copy, Debug)]
pub struct WalletBook {
    pub faction: u8,
    /// Fee + stake paid in.
    pub paid_in: u64,
    pub entitled: Claim,
    pub paid_out: u64,
    pub swept: u64,
}

/// The season's books (CL-07, CL-08): every conservation rule is checked
/// as a **bound** over these numbers (the verifier's V13 reuses the same
/// shapes), and the identities stay as a second check in `settle_run`.
#[derive(Clone, Debug)]
pub struct Books {
    pub prize: u64,
    pub cap_multiple: u64,
    /// Civilisation Share pot (shared by all factions).
    pub civ_pot: u64,
    /// Per faction `C_k + L_k`: the faction's citizen and laurel pots
    /// before the steward cut (the steward pot is 5% of it).
    pub faction_pot: [u64; FACTIONS],
    pub wallets: Vec<WalletBook>,
    /// Reward-index laurels: emitted by every province index, orphaned
    /// (no holder), credited to holdings (`Sim::distribute`).
    pub laurel_emitted: u128,
    pub laurel_orphaned: u128,
    pub laurel_credited: u128,
    /// Every laurel unit held at the end (wallets, reserves incl. burned,
    /// escrow, orphaned stakes, burned stakes) and every source
    /// (credited − free-city extra + relic minted).
    pub laurel_held: u128,
    pub laurel_sources: u128,
    /// Mandate reserves, summed over factions.
    pub mandate_deposited: u128,
    pub mandate_paid: u128,
    pub mandate_burned: u128,
    pub mandate_balance: u128,
}

/// The per-faction ledger of the simulator (CL-08, sim side): a claim
/// may not take faction `k` past its own pots, even while the season
/// total is fine. `payout::SeasonLedger` (W1-B) is the kernel form; this
/// book checks the same rule over the settled claims so the simulator and
/// the verifier see a cross-faction over-claim.
#[derive(Clone, Copy, Debug, Default)]
pub struct FactionBook {
    /// `C_k + L_k` + the civ shares the faction's wallets are entitled to.
    pub pot: [u128; FACTIONS],
    pub paid: [u128; FACTIONS],
    pub civ_pot: u128,
    pub civ: u128,
}

impl FactionBook {
    pub fn new(b: &Books) -> FactionBook {
        let mut fb = FactionBook {
            civ_pot: b.civ_pot as u128,
            ..FactionBook::default()
        };
        for (k, p) in b.faction_pot.iter().enumerate() {
            fb.pot[k] = *p as u128;
        }
        fb
    }

    /// Book one claim; refused (nothing changes) if faction `k` would pay
    /// out more than `C_k + L_k` plus the civ shares of its wallets, or
    /// the civ shares would pass the civ pot.
    pub fn record(&mut self, w: &WalletBook) -> Result<(), String> {
        let k = w.faction as usize;
        if k >= FACTIONS {
            return Err(format!("faction {k} out of range"));
        }
        let civ = self.civ + w.entitled.civ as u128;
        let pot = self.pot[k] + w.entitled.civ as u128;
        let paid = self.paid[k] + w.paid_out as u128;
        if civ > self.civ_pot {
            return Err(format!("civ shares {civ} > civ pot {}", self.civ_pot));
        }
        if paid > pot {
            return Err(format!("faction {k}: paid {paid} > its pots {pot}"));
        }
        self.civ = civ;
        self.pot[k] = pot;
        self.paid[k] = paid;
        Ok(())
    }
}

/// CL-07: conservation written as bounds. Returns one `Check` per bound;
/// `settle_run` adds the identities after them.
pub fn bound_checks(b: &Books) -> Vec<Check> {
    let mut v = Vec::new();
    // Money: every claim within its rule-computed entitlement and the cap.
    let over_entitled = b
        .wallets
        .iter()
        .filter(|w| w.paid_out > w.entitled.paid || w.swept > w.entitled.swept)
        .count();
    check(
        &mut v,
        "bound: each claim <= its rule-computed entitlement",
        over_entitled == 0,
        format!("{over_entitled} wallets paid above their entitlement"),
    );
    let over_cap = b
        .wallets
        .iter()
        .filter(|w| w.paid_out as u128 > w.paid_in as u128 * b.cap_multiple as u128)
        .count();
    check(
        &mut v,
        "bound: each claim <= 5x paid",
        over_cap == 0,
        format!("{over_cap} claims above {}x", b.cap_multiple),
    );
    let claimed: u128 = b.wallets.iter().map(|w| w.paid_out as u128).sum();
    let swept: u128 = b.wallets.iter().map(|w| w.swept as u128).sum();
    check(
        &mut v,
        "bound: claims + swept <= prize",
        claimed + swept <= b.prize as u128,
        format!("claimed {claimed} + swept {swept} <= prize {}", b.prize),
    );
    // CL-08: per faction.
    let mut fb = FactionBook::new(b);
    let refused: Vec<String> = b
        .wallets
        .iter()
        .filter_map(|w| fb.record(w).err())
        .collect();
    check(
        &mut v,
        "bound: each faction's claims <= its own pots (CL-08)",
        refused.is_empty(),
        match refused.first() {
            None => format!("per-faction paid {:?}", fb.paid),
            Some(e) => format!("{} refused, first: {e}", refused.len()),
        },
    );
    // Laurels.
    check(
        &mut v,
        "bound: laurels credited <= emitted - orphaned",
        b.laurel_credited + b.laurel_orphaned <= b.laurel_emitted,
        format!(
            "credited {} + orphaned {} <= emitted {}",
            b.laurel_credited, b.laurel_orphaned, b.laurel_emitted
        ),
    );
    check(
        &mut v,
        "bound: laurels held <= sources",
        b.laurel_held <= b.laurel_sources,
        format!("held {} <= sources {}", b.laurel_held, b.laurel_sources),
    );
    // Mandate reserves.
    check(
        &mut v,
        "bound: Mandate paid + burned + balance <= deposited",
        b.mandate_paid + b.mandate_burned + b.mandate_balance <= b.mandate_deposited,
        format!(
            "paid {} + burned {} + balance {} <= deposited {}",
            b.mandate_paid, b.mandate_burned, b.mandate_balance, b.mandate_deposited
        ),
    );
    v
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
                steward: match sim.cfg.office_pay {
                    OfficePay::Usdc => steward_rows(a.minister_terms, a.warden_terms),
                    // Variant: rows capped at a share of what was paid.
                    OfficePay::ShareOfPaid(bps) => steward_rows(a.minister_terms, a.warden_terms)
                        .min(((a.fee + a.stake) as u128 * bps as u128 / 10_000) as u64),
                    OfficePay::Laurels => 0,
                },
                voided: false,
            },
        ));
    }
    // FactionShards: every credit added; RevealShade removes each Shade.
    let pp: PayoutParams = sim.cfg.payout;
    let wpu = pp.works_per_usdc;
    let mut totals = [FactionTotals::default(); FACTIONS];
    for (_, r) in &recs {
        totals[r.faction as usize]
            .add(&r.weights_at(wpu))
            .expect("totals");
    }
    for (i, r) in recs.iter_mut() {
        if sim.agents[*i].shade {
            totals[r.faction as usize]
                .remove(&r.weights_at(wpu))
                .expect("void");
            r.voided = true;
        }
    }
    let facts = sim.faction_facts(true, sim.cfg.days);
    let index = faction_index(&facts, p);
    let members_all: u64 = facts.iter().map(|f| f.members).sum();
    let raw: [u64; FACTIONS] = core::array::from_fn(|k| clamped_mean(k, &facts, p));
    let herd: [u64; FACTIONS] = core::array::from_fn(|k| herding(facts[k].members, members_all, p));
    let st = settle(&sim.pools, &totals, &index, sim.engine_stages, &pp).expect("settle");
    let prize = sim.pools.prize().expect("prize");
    let mut ledger = SeasonLedger::new(&st).expect("ledger");
    assert_eq!(ledger.prize, prize);
    let mut agents = Vec::with_capacity(recs.len());
    let mut checks = Vec::new();
    let mut over_cap = 0u64;
    let mut office_profit = 0u64;
    let mut voided_paid = 0u64;
    let mut sum_paid = 0u128;
    let mut hash_in: Vec<u8> = Vec::with_capacity(recs.len() * 24);
    let mut wallets = Vec::with_capacity(recs.len());
    for (i, r) in &recs {
        let c = claim(&st, r).expect("claim");
        ledger.record(r.faction, &c).expect("ledger");
        wallets.push(WalletBook {
            faction: r.faction,
            paid_in: r.paid(),
            entitled: claim(&st, r).expect("entitlement"),
            paid_out: c.paid,
            swept: c.swept,
        });
        let a = &sim.agents[*i];
        if c.paid > 5 * r.paid() {
            over_cap += 1;
        }
        // O3: officer pay never lifts a claim above the ceiling.
        if pp.office_ceiling_bps != u32::MAX
            && c.steward > 0
            && c.paid as u128 * 10_000 > r.paid() as u128 * pp.office_ceiling_bps as u128
        {
            office_profit += 1;
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
    // ---- CL-07/CL-08: bounds first, identities after.
    let (emitted, orphaned) = sim.laurels_minted();
    let held: u128 = sim.agents.iter().map(|a| a.laurels as u128).sum::<u128>()
        + sim
            .reserve
            .iter()
            .map(|r| r.balance as u128 + r.burned)
            .sum::<u128>()
        + sim.escrow as u128
        + sim.stats.siege_stake_orphaned as u128
        + sim.stats.laurels_burned as u128;
    let sources = sim.laurel_credited - sim.laurel_orphan_extra + sim.stats.relic_minted as u128;
    let books = Books {
        prize,
        cap_multiple: st.cap_multiple,
        civ_pot: st.civ_pot,
        faction_pot: core::array::from_fn(|k| st.factions[k].citizen + st.factions[k].laurel),
        wallets,
        laurel_emitted: emitted,
        laurel_orphaned: orphaned,
        laurel_credited: sim.laurel_credited,
        laurel_held: held,
        laurel_sources: sources,
        mandate_deposited: sim.reserve.iter().map(|r| r.deposited).sum(),
        mandate_paid: sim.reserve.iter().map(|r| r.paid).sum(),
        mandate_burned: sim.reserve.iter().map(|r| r.burned).sum(),
        mandate_balance: sim.reserve.iter().map(|r| r.balance as u128).sum(),
    };
    checks.extend(bound_checks(&books));
    // ---- money conservation (design §5.4 F1), the identities
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
        "officer pay within its ceiling",
        office_profit == 0,
        format!("{office_profit} officers paid above the ceiling"),
    );
    check(
        &mut checks,
        "voided Shades paid nothing",
        voided_paid == 0,
        format!("{voided_paid} voided claims > 0"),
    );
    // ---- laurel conservation (zero-sum emission, §5.4)
    check(
        &mut checks,
        "laurels: held = credited + relic minted",
        held == sources,
        format!("held {held} sources {sources}"),
    );
    // Mandate reserve (O10): deposited = balance + paid + burned (the m0c
    // share floor's cut), every term swept.
    let (dep, bal, rpaid, open, burned) =
        sim.reserve
            .iter()
            .fold((0u128, 0u128, 0u128, 0u128, 0u128), |a, r| {
                (
                    a.0 + r.deposited,
                    a.1 + r.balance as u128,
                    a.2 + r.paid,
                    a.3 + r.outstanding().expect("outstanding"),
                    a.4 + r.burned,
                )
            });
    check(
        &mut checks,
        "Mandate reserve: deposited = balance + paid + burned",
        dep == bal + rpaid + burned && open == 0 && rpaid == sim.stats.mandate_paid as u128,
        format!(
            "deposited {dep} = balance {bal} + paid {rpaid} + floor burned {burned}; open {open}"
        ),
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
        books,
        checks,
        digest: sha256(&[b"sim/digest", &hash_in]),
    }
}
