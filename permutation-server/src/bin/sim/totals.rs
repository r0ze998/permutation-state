//! Totals over all seasons and the summary against the V5 targets.

use crate::env::Env;
use crate::season::{Season, Sec18};
use crate::seed_bytes;
use permutation_rules::checks::BLOCKED_NAMES;
use permutation_rules::genesis::start_order;
use permutation_rules::scoring::PATH_NAMES;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Totals {
    /// Era of every nation with members.
    eras: Vec<u8>,
    era5: u32,
    era_by_persona: BTreeMap<&'static str, Vec<u8>>,
    tier_counts: [[u32; 6]; 4],
    top_share: Vec<f64>,
    over40: u32,
    /// Top two paths of each era-3+ nation (legacy metric).
    pairs: BTreeMap<(usize, usize), u32>,
    dead_points: Vec<u64>,
    off_pay: u64,
    off_n: u64,
    non_pay: u64,
    non_n: u64,
    /// Nation size -> (USDC paid, members).
    per_member_nation: BTreeMap<usize, (u64, u64)>,
    adopted: u32,
    recalls: u32,
    budget: f64,
    violations: usize,
    merit_paths: [u64; 5],
    /// Every pair of paths at tier 3+ in an era-3+ nation (a nation counts
    /// for each pair it has), and how many era-3+ nations have science 3+.
    pairs_all: BTreeMap<(usize, usize), u32>,
    era3: u32,
    era3_sci: u32,
    era_ticks: [Vec<u16>; 3],
    lead_known: u32,
    lead_changed: u32,
    wars: u32,
    captures: u32,
    hub_holders: usize,
    hub_dominated: u32,
    chivalry: u32,
    counted_n: u32,
    /// By generation slot: (points, seasons), cities, skipped orders.
    slot_points: Vec<(u64, u32)>,
    slot_cities: Vec<u64>,
    slot_skips: Vec<[u64; 64]>,
    s18: Vec<Sec18>,
}

fn median_tick(v: &mut [u16]) -> Option<u16> {
    v.sort();
    v.get(v.len() / 2).copied()
}

impl Totals {
    pub fn new(nations: usize) -> Totals {
        Totals {
            slot_points: vec![(0, 0); nations],
            slot_cities: vec![0; nations],
            slot_skips: vec![[0; 64]; nations],
            ..Default::default()
        }
    }

    /// Adds season `i` (taking its §18 record).
    pub fn add(&mut self, i: u32, r: &mut Season) {
        self.s18.push(std::mem::take(&mut r.s18));
        let counted: Vec<usize> = (0..r.members.len()).filter(|c| r.members[*c] > 0).collect();
        for &c in &counted {
            self.eras.push(r.era[c]);
            self.era5 += (r.era[c] == 5) as u32;
            self.era_by_persona
                .entry(r.persona[c].name())
                .or_default()
                .push(r.era[c]);
            for (p, counts) in self.tier_counts.iter_mut().enumerate() {
                counts[r.tiers[c][p] as usize] += 1;
            }
            // Which two paths carried the nation to its era (legacy metric:
            // ties go to the higher path index, one pair per nation).
            let mut t: Vec<(u8, usize)> = r.tiers[c].iter().copied().zip(0..4).collect();
            t.sort_by(|a, b| b.cmp(a));
            if r.era[c] >= 3 {
                let (a, b) = (t[0].1.min(t[1].1), t[0].1.max(t[1].1));
                *self.pairs.entry((a, b)).or_default() += 1;
                self.era3 += 1;
                self.era3_sci += (r.tiers[c][2] >= 3) as u32;
                for a in 0..4 {
                    for b in a + 1..4 {
                        if r.tiers[c][a] >= 3 && r.tiers[c][b] >= 3 {
                            *self.pairs_all.entry((a, b)).or_default() += 1;
                        }
                    }
                }
            }
            for (k, list) in self.era_ticks.iter_mut().enumerate() {
                list.extend(r.era_tick[c][k]);
            }
            self.counted_n += 1;
            self.chivalry += r.chivalry_t150[c] as u32;
        }
        // By generation slot (nation c starts at slot order[c], §2.4).
        let order = start_order(&seed_bytes("season", i), r.members.len());
        for (c, slot) in order.iter().enumerate() {
            self.slot_points[*slot].0 += r.points[c];
            self.slot_points[*slot].1 += 1;
            self.slot_cities[*slot] += r.cities[c] as u64;
            for (k, x) in r.skips[c].iter().enumerate() {
                self.slot_skips[*slot][k] += *x as u64;
            }
        }
        if let (Some(a), Some(b)) = (r.leader_t120, r.leader_end) {
            self.lead_known += 1;
            self.lead_changed += (a != b) as u32;
        }
        self.wars += r.wars;
        self.captures += r.captures;
        self.hub_holders += r.hub_holders;
        self.hub_dominated += (r.hub_hold_max > 0.8) as u32;
        for c in 0..r.members.len() {
            if r.cities[c] == 0 {
                self.dead_points.push(r.points[c]);
            }
        }
        let top = r.share.iter().cloned().fold(0.0, f64::max);
        self.top_share.push(top);
        if top > 0.4 {
            self.over40 += 1;
        }
        for (civ, officer, _merit, pay) in &r.member_pay {
            if *officer {
                self.off_pay += pay;
                self.off_n += 1;
            } else {
                self.non_pay += pay;
                self.non_n += 1;
            }
            let e = self
                .per_member_nation
                .entry(r.members[*civ as usize])
                .or_default();
            e.0 += pay;
            e.1 += 1;
        }
        self.adopted += r.adopted;
        self.recalls += r.recalls;
        self.budget += r.budget_use;
        self.violations += r.violations;
        for (total, m) in self.merit_paths.iter_mut().zip(r.merit_by_path) {
            *total += m;
        }
    }

    /// The summary against the V5 §6.5 targets, then V5 §18.
    pub fn print(&mut self, seeds: u32, env: &Env) {
        let Totals {
            over40,
            lead_changed,
            lead_known,
            hub_dominated,
            violations,
            ..
        } = *self;
        self.eras.sort();
        let eras = &self.eras;
        let median = eras.get(eras.len() / 2).copied().unwrap_or(0);
        println!("\n== {seeds} seasons, V5 §6.5 targets ==");
        println!(
            "era of nations with members: median {median} (target 2–3), distribution {:?}",
            (0..=5)
                .map(|e| eras.iter().filter(|x| **x == e).count())
                .collect::<Vec<_>>()
        );
        println!(
            "era 5 per season: {:.2} (target 0–1)",
            self.era5 as f64 / seeds as f64
        );
        println!(
            "top nation's pool share > 40%: {over40}/{seeds} (target: rare); mean top share {:.0}%",
            100.0 * self.top_share.iter().sum::<f64>() / self.top_share.len().max(1) as f64
        );
        let pair_names: Vec<String> = self
            .pairs
            .iter()
            .map(|((a, b), n)| format!("{}+{} {}", PATH_NAMES[*a], PATH_NAMES[*b], n))
            .collect();
        println!(
            "path pairs that reached era 3+ (legacy, exclusive): {} (target: every pair possible)",
            if pair_names.is_empty() {
                "none".into()
            } else {
                pair_names.join(", ")
            }
        );
        let pct = |a: u32, b: u32| 100.0 * a as f64 / b.max(1) as f64;
        let era3 = self.era3;
        println!(
            "path pairs at tier 3+ in era-3+ nations (every pair a nation has, % of {era3}): {}",
            (0..4)
                .flat_map(|a| (a + 1..4).map(move |b| (a, b)))
                .map(|(a, b)| {
                    let k = self.pairs_all.get(&(a, b)).copied().unwrap_or(0);
                    format!(
                        "{}+{} {k} ({:.0}%)",
                        PATH_NAMES[a],
                        PATH_NAMES[b],
                        pct(k, era3)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "era-3+ nations with science 3+: {:.0}% (target ≤ 70%)",
            pct(self.era3_sci, era3)
        );
        println!(
            "first tick in era 1/2/3, median: {}",
            self.era_ticks
                .iter_mut()
                .map(|v| median_tick(v).map_or("-".into(), |t| format!("{t} (n={})", v.len())))
                .collect::<Vec<String>>()
                .join(" / ")
        );
        println!(
            "leader at T120 ≠ final leader: {lead_changed}/{lead_known} seasons (target ≥ 25%)"
        );
        let per = |x: f64| x / seeds.max(1) as f64;
        println!(
            "per season: wars declared {:.1}, cities captured {:.1}, nations that held a hub {:.1}; seasons with one nation holding a hub > 80% of ticks: {hub_dominated}/{seeds}",
            per(self.wars as f64),
            per(self.captures as f64),
            per(self.hub_holders as f64),
        );
        println!(
            "nations with members that had Chivalry by T150: {:.0}%",
            pct(self.chivalry, self.counted_n)
        );
        println!(
            "mean points by generation slot (map fairness; nations are shuffled over slots): {}",
            self.slot_points
                .iter()
                .enumerate()
                .map(|(c, (sum, k))| format!("{c}:{:.0}", *sum as f64 / (*k).max(1) as f64))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!(
            "mean cities by generation slot: {}",
            self.slot_cities
                .iter()
                .zip(&self.slot_points)
                .enumerate()
                .map(|(c, (n, (_, k)))| format!("{c}:{:.2}", *n as f64 / (*k).max(1) as f64))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if env.skips {
            self.print_skips();
        }
        println!("dead nations' points: {:?} (target ≈ 0)", self.dead_points);
        for (p, name) in PATH_NAMES.iter().enumerate() {
            println!("  {name:10} tiers 0..5: {:?}", self.tier_counts[p]);
        }
        for (p, v) in &self.era_by_persona {
            let avg = v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64;
            println!("  {p:9} mean era {avg:.2} over {} nations", v.len());
        }
        println!(
            "payout per member (USDC): officers {:.2}, others {:.2}; by nation size {}",
            self.off_pay as f64 / 1e6 / self.off_n.max(1) as f64,
            self.non_pay as f64 / 1e6 / self.non_n.max(1) as f64,
            self.per_member_nation
                .iter()
                .map(|(k, (sum, n))| format!("{k}:{:.2}", *sum as f64 / 1e6 / *n as f64))
                .collect::<Vec<_>>()
                .join(" "),
        );
        let tm: u64 = self.merit_paths.iter().sum();
        println!(
            "merit by path: {} ",
            ["hegemony", "prosperity", "science", "concord", "common"]
                .iter()
                .zip(self.merit_paths)
                .map(|(n, m)| format!("{n} {:.0}%", 100.0 * m as f64 / tm.max(1) as f64))
                .collect::<Vec<_>>()
                .join("  ")
        );
        println!(
            "proposals adopted: {:.1} per season; office changes mid-term (recalls): {:.1} per season",
            self.adopted as f64 / seeds as f64,
            self.recalls as f64 / seeds as f64
        );
        println!(
            "order budget used: {:.0}%   invariant violations: {violations}",
            100.0 * self.budget / seeds.max(1) as f64
        );
        print_sec18(&self.s18, env);
    }

    /// `SIM_SKIPS=1`: the most frequent skipped orders per generation slot.
    fn print_skips(&self) {
        for (slot, row) in self.slot_skips.iter().enumerate() {
            let mut top: Vec<(u64, usize)> = row
                .iter()
                .enumerate()
                .filter(|(_, x)| **x > 0)
                .map(|(k, x)| (*x, k))
                .collect();
            top.sort_by(|a, b| b.cmp(a));
            println!(
                "  slot {slot} skipped orders: {}",
                top.iter()
                    .take(6)
                    .map(|(x, k)| format!("{} {x}", BLOCKED_NAMES.get(*k).unwrap_or(&"?")))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

/// V5 §18 summary: AI homes and bounties, redistribution, contracts.
fn print_sec18(s18: &[Sec18], env: &Env) {
    let k = s18.len().max(1) as f64;
    let sum = |f: &dyn Fn(&Sec18) -> u64| s18.iter().map(f).sum::<u64>();
    let top40 = s18
        .iter()
        .filter(|x| x.share.iter().cloned().fold(0.0, f64::max) > 0.4)
        .count();
    let with = sum(&|x| x.people_with);
    let without = sum(&|x| x.people_without);
    println!(
        "\n== V5 §18 (SIM_AI operator AI members per nation, SIM_BOUNTY each, home tick {}) ==",
        env.rules().ai_home_tick
    );
    println!(
        "AI homes conquered: {:.2} per season of {:.1} AIs ({:.0}%); voided by a recent pact {:.2}; bounty paid {:.2} USDC per season",
        sum(&|x| x.homes_paid as u64) as f64 / k,
        sum(&|x| x.ais as u64) as f64 / k,
        100.0 * sum(&|x| x.homes_paid as u64) as f64 / sum(&|x| x.ais as u64).max(1) as f64,
        sum(&|x| x.homes_voided as u64) as f64 / k,
        sum(&|x| x.bounty_total) as f64 / k / 1e6
    );
    println!(
        "top nation's share (bounties included) > 40%: {top40}/{}; AI payouts redistributed {:.2} USDC per season; people receive {:+.1}% vs no roster",
        s18.len(),
        sum(&|x| x.redistributed) as f64 / k / 1e6,
        if without == 0 { 0.0 } else { 100.0 * (with as f64 - without as f64) / without as f64 }
    );
    println!(
        "contracts per season: {:.2} offered, {:.2} accepted, {:.2} USDC paid (SIM_TREASURY per nation)",
        sum(&|x| x.contracts_offered as u64) as f64 / k,
        sum(&|x| x.contracts_accepted as u64) as f64 / k,
        sum(&|x| x.contract_paid) as f64 / k / 1e6
    );
}
