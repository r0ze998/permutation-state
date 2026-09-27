//! Tables for RESULTS.md: payout multiples, faction indices, laurel
//! sources, conservation checks.

use std::fmt::Write;

use crate::model::{Arch, ARCHS};
use crate::settle::{AgentOut, Outcome};
use crate::sim::{Sim, LAUREL};
use permutation_rules::frontier::index::INDEX_ONE;
use permutation_rules::frontier::pools::USDC;

/// Σ claims / Σ paid over a group, and its size.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mult {
    pub n: u64,
    pub paid: u128,
    pub got: u128,
}

impl Mult {
    pub fn add(&mut self, a: &AgentOut) {
        self.n += 1;
        self.paid += a.paid() as u128;
        self.got += a.claim.paid as u128;
    }
    pub fn merge(&mut self, o: &Mult) {
        self.n += o.n;
        self.paid += o.paid;
        self.got += o.got;
    }
    pub fn x(&self) -> f64 {
        if self.paid == 0 {
            f64::NAN
        } else {
            self.got as f64 / self.paid as f64
        }
    }
    pub fn cell(&self) -> String {
        if self.n == 0 {
            "–".into()
        } else if self.n < 20 {
            format!("{:.2} (n={})", self.x(), self.n)
        } else {
            format!("{:.2}", self.x())
        }
    }
}

pub fn fx(x: u64) -> f64 {
    x as f64 / INDEX_ONE as f64
}

pub fn usdc(x: u64) -> f64 {
    x as f64 / USDC as f64
}

pub fn lau(x: u64) -> f64 {
    x as f64 / LAUREL as f64
}

pub const DAY_BUCKETS: [(u32, u32, &str); 4] = [
    (0, 0, "day 0"),
    (1, 7, "days 1-7"),
    (8, 14, "days 8-14"),
    (15, 21, "days 15-21"),
];

/// `[arch][staker][bucket]`, humans and bots; Shades excluded.
pub type Grid = [[[Mult; 4]; 2]; 6];

pub fn grid(outs: &[&Outcome]) -> Grid {
    let mut g: Grid = Default::default();
    for o in outs {
        for a in o.agents.iter().filter(|a| !a.shade) {
            let bk = DAY_BUCKETS
                .iter()
                .position(|(lo, hi, _)| a.join_day >= *lo && a.join_day <= *hi)
                .unwrap_or(3);
            g[a.arch.idx()][(a.stake > 0) as usize][bk].add(a);
        }
    }
    g
}

pub fn payout_table(outs: &[&Outcome]) -> String {
    let g = grid(outs);
    let mut s = String::new();
    writeln!(
        s,
        "| Archetype | wallets | citizen fee only (x fee) | with laurel stake (x fee+stake) | stakers |"
    )
    .unwrap();
    writeln!(s, "|---|---|---|---|---|").unwrap();
    for a in ARCHS {
        let mut c = Mult::default();
        let mut l = Mult::default();
        for bk in 0..4 {
            c.merge(&g[a.idx()][0][bk]);
            l.merge(&g[a.idx()][1][bk]);
        }
        writeln!(
            s,
            "| {} | {} | {} | {} | {} |",
            a.name(),
            c.n + l.n,
            c.cell(),
            l.cell(),
            l.n
        )
        .unwrap();
    }
    s
}

pub fn join_day_table(outs: &[&Outcome]) -> String {
    let g = grid(outs);
    let mut s = String::new();
    write!(s, "| Archetype | stake |").unwrap();
    for (_, _, n) in DAY_BUCKETS {
        write!(s, " {n} |").unwrap();
    }
    writeln!(s).unwrap();
    writeln!(s, "|---|---|---|---|---|---|").unwrap();
    for a in ARCHS {
        for st in 0..2 {
            write!(
                s,
                "| {} | {} |",
                a.name(),
                if st == 1 { "yes" } else { "no" }
            )
            .unwrap();
            for bk in 0..4 {
                write!(s, " {} |", g[a.idx()][st][bk].cell()).unwrap();
            }
            writeln!(s).unwrap();
        }
    }
    s
}

pub fn faction_table(sim: &Sim, o: &Outcome) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "| Faction | doctrine | members | active | Dominion/cap | Prosperity/cap | Knowledge/cap | Concord/cap | raw index | h_k | s_k | claims / paid |"
    )
    .unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for k in 0..6 {
        let f = &o.facts[k];
        let mut m = Mult::default();
        for a in o
            .agents
            .iter()
            .filter(|a| a.faction as usize == k && !a.shade)
        {
            m.add(a);
        }
        let pc = |p: usize| f.path[p] as f64 / f.active.max(1) as f64;
        writeln!(
            s,
            "| {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.3} | {:.3} | {:.3} | {:.3} |",
            k,
            if sim.cfg.doctrines {
                sim.doctrine[k].name()
            } else {
                "-"
            },
            f.members,
            f.active,
            pc(0),
            pc(1),
            pc(2),
            pc(3),
            fx(o.raw[k]),
            fx(o.herd[k]),
            fx(o.index[k]),
            m.x()
        )
        .unwrap();
    }
    s
}

pub fn laurel_table(outs: &[&Outcome]) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "| Archetype | laurels banked / wallet | holding share | occupation | relic | mandate | captures in | captures out | siege stakes net | works / wallet | sessions / wallet |"
    )
    .unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for a in ARCHS {
        let mut n = 0u64;
        let mut tot = [0f64; 10];
        for o in outs {
            for x in o.agents.iter().filter(|x| x.arch == a && !x.shade) {
                n += 1;
                tot[0] += lau(x.laurels);
                tot[1] += lau(x.earned.holding);
                tot[2] += lau(x.earned.occupation);
                tot[3] += lau(x.earned.relic);
                tot[4] += lau(x.earned.mandate);
                tot[5] += lau(x.earned.capture_in);
                tot[6] += lau(x.earned.capture_out);
                tot[7] += x.earned.siege_net as f64 / LAUREL as f64;
                tot[8] += x.works as f64;
                tot[9] += x.sessions as f64;
            }
        }
        let n = n.max(1) as f64;
        write!(s, "| {} |", a.name()).unwrap();
        for v in tot {
            write!(s, " {:.1} |", v / n).unwrap();
        }
        writeln!(s).unwrap();
    }
    s
}

pub fn checks_table(o: &Outcome) -> String {
    let mut s = String::new();
    writeln!(s, "| Check | result | detail |").unwrap();
    writeln!(s, "|---|---|---|").unwrap();
    for c in &o.checks {
        writeln!(
            s,
            "| {} | {} | {} |",
            c.name,
            if c.ok { "PASS" } else { "FAIL" },
            c.detail
        )
        .unwrap();
    }
    s
}

pub fn stats_table(sim: &Sim, o: &Outcome) -> String {
    let st = &sim.stats;
    let mut s = String::new();
    let holdings = sim
        .holds
        .iter()
        .filter(|h| h.alive && h.owner != crate::sim::NONE)
        .count();
    let free_cities = sim
        .holds
        .iter()
        .filter(|h| h.owner == crate::sim::NONE)
        .count();
    let tiers = {
        let mut t = [0u64; 4];
        for h in sim
            .holds
            .iter()
            .filter(|h| h.alive && h.owner != crate::sim::NONE)
        {
            t[h.h.tier as usize] += 1;
        }
        t
    };
    let rows: Vec<(&str, String)> = vec![
        (
            "wallets joined / withdrawn (no site)",
            format!("{} / {}", sim.agents.len(), st.withdrawn_joins),
        ),
        ("paid in (USDC)", format!("{:.2}", usdc(sim.paid_in))),
        (
            "prize pools (citizen + laurel)",
            format!("{:.2} + {:.2}", usdc(o.pools.citizen), usdc(o.pools.laurel)),
        ),
        (
            "final ring / provinces opened / sites used of open",
            format!(
                "{} / {} rings / {} of {}",
                st.final_ring, st.rings_opened, sim.used_sites, sim.open_sites
            ),
        ),
        (
            "holdings (Hamlet/Town/City/Stronghold); Free Cities",
            format!(
                "{} ({}/{}/{}/{}); {}",
                holdings, tiers[0], tiers[1], tiers[2], tiers[3], free_cities
            ),
        ),
        ("2nd/3rd holdings founded", st.second_holdings.to_string()),
        ("sessions", st.sessions.to_string()),
        (
            "clashes resolved / engagements / kernel refusals",
            format!("{} / {} / {}", st.clashes, st.engagements, st.clash_errors),
        ),
        (
            "sieges declared / completed / failed",
            format!(
                "{} / {} / {}",
                st.sieges_declared, st.sieges_completed, st.sieges_failed
            ),
        ),
        (
            "siege failures: never held / lost (host destroyed, host left)",
            format!(
                "{} / {} ({}, {})",
                st.fail_never_held, st.fail_lost, st.lost_destroyed, st.lost_left
            ),
        ),
        (
            "occupations / liberations / captures / Free City captures",
            format!(
                "{} / {} / {} / {}",
                st.occupations, st.liberations, st.captures, st.free_city_captures
            ),
        ),
        ("camps beaten", st.camps_won.to_string()),
        (
            "routs (unrevealed) / Disarray postures",
            format!("{} / {}", st.routs, st.disarray),
        ),
        (
            "Engine stages (completion days)",
            format!("{} {:?}", sim.engine_stages, st.engine_stage_days),
        ),
        (
            "Relic Sites spawned / laurels minted by relics",
            format!("{} / {:.0}", st.relics_spawned, lau(st.relic_minted)),
        ),
        (
            "dormancies / first holdings released",
            format!("{} / {}", st.dormancies, st.releases),
        ),
        (
            "Mandate reserve paid / left",
            format!("{:.0} / {:.0}", lau(st.mandate_paid), lau(st.mandate_left)),
        ),
        (
            "Mandate shares (staker completions) / fee-only completions (no share)",
            format!("{} / {}", st.mandate_shares, st.mandate_unshared),
        ),
        (
            "office-terms: Minister / paid Warden / held by bots",
            format!(
                "{} / {} / {}",
                st.minister_terms, st.warden_terms, st.bot_office_terms
            ),
        ),
        (
            "vacant seats (D23): Minister / Warden",
            format!("{} / {}", st.minister_vacant, st.warden_vacant),
        ),
        (
            "officer pay cut by the ceiling (USDC, swept)",
            format!(
                "{:.2}",
                usdc(o.agents.iter().map(|a| a.claim.office_cut).sum::<u64>())
            ),
        ),
        (
            "builders (Civilisation Share)",
            o.agents.iter().filter(|a| a.builder).count().to_string(),
        ),
        (
            "civ pot / steward pots (USDC)",
            format!(
                "{:.2} / {:.2}",
                usdc(o.settlement.civ_pot),
                usdc(o.settlement.factions.iter().map(|f| f.steward_paid()).sum())
            ),
        ),
        (
            "claimed / swept / dust (USDC)",
            format!(
                "{:.2} / {:.2} / {:.6}",
                usdc(o.ledger.claimed),
                usdc(o.ledger.swept),
                usdc(o.ledger.dust())
            ),
        ),
        (
            "claims by faction / its pots C+L (USDC, CL-08)",
            (0..6)
                .map(|k| {
                    let paid: u64 = o
                        .books
                        .wallets
                        .iter()
                        .filter(|w| w.faction as usize == k)
                        .map(|w| w.paid_out)
                        .sum();
                    format!("{:.0}/{:.0}", usdc(paid), usdc(o.books.faction_pot[k]))
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ];
    writeln!(s, "| Quantity | value |").unwrap();
    writeln!(s, "|---|---|").unwrap();
    for (k, v) in rows {
        writeln!(s, "| {k} | {v} |").unwrap();
    }
    s
}

/// Per-capita payout of faction 0 relative to factions 1..6, as a ratio
/// of Σ claims / Σ paid, over all citizens, non-stakers and stakers.
pub fn herding_ratio(o: &Outcome) -> (f64, f64, f64) {
    let mut big = [Mult::default(); 3];
    let mut small = [Mult::default(); 3];
    for a in o.agents.iter().filter(|a| !a.shade) {
        let g = if a.faction == 0 { &mut big } else { &mut small };
        g[0].add(a);
        g[1 + (a.stake > 0) as usize].add(a);
    }
    (
        big[0].x() / small[0].x(),
        big[1].x() / small[1].x(),
        big[2].x() / small[2].x(),
    )
}

pub fn arch_mult(o: &Outcome, arch: Arch, staker: bool) -> Mult {
    let mut m = Mult::default();
    for a in o
        .agents
        .iter()
        .filter(|a| a.arch == arch && !a.shade && (a.stake > 0) == staker)
    {
        m.add(a);
    }
    m
}

pub fn hex(d: &[u8; 32]) -> String {
    d.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Mean claim parts (USDC) per wallet, by archetype and stake.
pub fn claim_parts_table(outs: &[&Outcome]) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "| Archetype | stake | wallets | paid | citizen pool | Civilisation Share | laurel pool | steward | total claim | multiple |"
    )
    .unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for a in ARCHS {
        for st in [false, true] {
            let mut n = 0f64;
            let mut t = [0f64; 6];
            for o in outs {
                for x in o
                    .agents
                    .iter()
                    .filter(|x| x.arch == a && !x.shade && (x.stake > 0) == st)
                {
                    n += 1.0;
                    t[0] += usdc(x.paid());
                    t[1] += usdc(x.claim.citizen);
                    t[2] += usdc(x.claim.civ);
                    t[3] += usdc(x.claim.laurel);
                    t[4] += usdc(x.claim.steward);
                    t[5] += usdc(x.claim.paid);
                }
            }
            if n == 0.0 {
                continue;
            }
            writeln!(
                s,
                "| {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
                a.name(),
                if st { "yes" } else { "no" },
                n,
                t[0] / n,
                t[1] / n,
                t[2] / n,
                t[3] / n,
                t[4] / n,
                t[5] / n,
                t[5] / t[0]
            )
            .unwrap();
        }
    }
    s
}

/// First-holding tier at T_end and holdings per wallet, by archetype.
pub fn tier_table(outs: &[&Outcome]) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "| Archetype | Hamlet | Town | City | Stronghold | none (released) | holdings / wallet |"
    )
    .unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|").unwrap();
    for a in ARCHS {
        let mut c = [0f64; 5];
        let mut h = 0f64;
        let mut n = 0f64;
        for o in outs {
            for x in o.agents.iter().filter(|x| x.arch == a && !x.shade) {
                c[x.first_tier as usize] += 1.0;
                h += x.holdings as f64;
                n += 1.0;
            }
        }
        let n = n.max(1.0);
        writeln!(
            s,
            "| {} | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.0}% | {:.2} |",
            a.name(),
            100.0 * c[0] / n,
            100.0 * c[1] / n,
            100.0 * c[2] / n,
            100.0 * c[3] / n,
            100.0 * c[4] / n,
            h / n
        )
        .unwrap();
    }
    s
}

/// Laurels banked per staked USDC and per day available, by join bucket
/// (stakers only; explains the join-day pattern).
pub fn join_laurel_table(outs: &[&Outcome]) -> String {
    let mut s = String::new();
    writeln!(s, "| Join | stakers | laurels / wallet | per day available: all | holding share | Mandates | relics | other | laurels per staked USDC | laurel-pool claim / stake | first holding Stronghold |").unwrap();
    writeln!(s, "|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (lo, hi, name) in DAY_BUCKETS {
        let mut v = [0f64; 10];
        for o in outs {
            for x in o.agents.iter().filter(|x| {
                !x.shade
                    && x.stake > 0
                    && x.join_day >= lo
                    && x.join_day <= hi
                    && x.arch != Arch::Idle
            }) {
                let days = (28 - x.join_day) as f64;
                v[0] += 1.0;
                v[1] += lau(x.laurels);
                v[2] += lau(x.laurels) / days;
                v[3] += lau(x.earned.holding) / days;
                v[4] += lau(x.earned.mandate) / days;
                v[5] += lau(x.earned.relic) / days;
                v[6] += (lau(x.laurels)
                    - lau(x.earned.holding)
                    - lau(x.earned.mandate)
                    - lau(x.earned.relic))
                    / days;
                v[7] += usdc(x.stake);
                v[8] += usdc(x.claim.laurel);
                v[9] += (x.first_tier == 3) as u8 as f64;
            }
        }
        let n = v[0];
        if n == 0.0 {
            continue;
        }
        writeln!(
            s,
            "| {name} | {n} | {:.1} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} | {:.2} | {:.0}% |",
            v[1] / n,
            v[2] / n,
            v[3] / n,
            v[4] / n,
            v[5] / n,
            v[6] / n,
            v[1] / v[7],
            v[8] / v[7],
            100.0 * v[9] / n
        )
        .unwrap();
    }
    s
}

/// Where first holdings sit, by join bucket (stakers, idle excluded).
pub fn neighbourhood_table(outs: &[&Outcome]) -> String {
    let mut s = String::new();
    writeln!(s, "| Join | first holdings | attached holdings in its province | of which first holdings | own weight / province mean |").unwrap();
    writeln!(s, "|---|---|---|---|---|").unwrap();
    for (lo, hi, name) in DAY_BUCKETS {
        let (mut n, mut a, mut f, mut r, mut nr) = (0f64, 0f64, 0f64, 0f64, 0f64);
        for o in outs {
            for x in o.agents.iter().filter(|x| {
                !x.shade
                    && x.stake > 0
                    && x.join_day >= lo
                    && x.join_day <= hi
                    && x.arch != Arch::Idle
            }) {
                if x.nbhd.0 == 0 {
                    continue;
                }
                n += 1.0;
                a += x.nbhd.0 as f64;
                f += x.nbhd.1 as f64;
                if x.nbhd.2.is_finite() {
                    r += x.nbhd.2;
                    nr += 1.0;
                }
            }
        }
        if n == 0.0 {
            continue;
        }
        writeln!(
            s,
            "| {name} | {n} | {:.2} | {:.2} | {:.2} |",
            a / n,
            f / n,
            r / nr.max(1.0)
        )
        .unwrap();
    }
    s
}
