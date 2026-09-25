//! Treasury contracts (V5 §18.6), by temperament.

use permutation_rules::orders::Order;
use permutation_rules::state::{CivId, Relation, WorldState};
use permutation_rules::Ruleset;

use super::geo::troops_of;
use super::Bot;

impl Bot {
    /// Treasury contracts (V5 §18.6): answer the offers made to this nation,
    /// and offer some, by temperament. Words bind nothing; only escrowed
    /// USDC moves anyone.
    pub(super) fn contracts(&self, s: &WorldState, r: &Ruleset, wish: &mut Vec<(u8, Order)>) {
        use permutation_rules::contracts::ContractTerm;
        if !r.market_enabled {
            return;
        }
        let civ = self.civ;
        let t = self.traits;
        let me = &s.civs[civ as usize];
        let n = s.civs.len() as CivId;
        // Answer offers: take them when they pay enough and the term suits.
        for c in s
            .contracts
            .iter()
            .filter(|c| c.to == Some(civ) && c.accepted.is_none() && c.offered < s.tick)
        {
            if t.openness < 20 || c.total < t.price() {
                continue;
            }
            let suits = match c.term {
                ContractTerm::Peace => {
                    t.aggression < 70 || troops_of(s, civ) <= troops_of(s, c.from)
                }
                ContractTerm::KeepNap { .. } => t.aggression < 60,
                ContractTerm::LeaveAlliance { .. } => t.loyalty < 50 && c.total >= t.price() * 2,
                ContractTerm::Capture { .. } => false,
            };
            if suits {
                wish.push((0, Order::AcceptContract { id: c.id }));
            }
        }
        // What the treasury may still put in escrow (not contract income).
        let spendable = me.free_usdc();
        let open = permutation_rules::contracts::open_offers(s, civ);
        if spendable < 1_000_000
            || open >= r.contract_max_open as usize
            || s.tick >= r.exchange_freeze_tick
        {
            return;
        }
        let last = r.ticks_per_season.saturating_sub(1);
        let offered_to = |o: CivId| s.contracts.iter().any(|c| c.from == civ && c.to == Some(o));
        // Losing a war: buy peace.
        for o in 0..n {
            if o != civ
                && s.at_war(civ, o)
                && troops_of(s, civ) < troops_of(s, o)
                && !offered_to(o)
                && t.aggression < 85
            {
                wish.push((
                    1,
                    Order::OfferContract {
                        to: Some(o),
                        term: ContractTerm::Peace,
                        usdc: (spendable / 2).min(3_000_000),
                        deadline: (s.tick + 20).min(last),
                    },
                ));
                return;
            }
        }
        // Aggressive and well funded: a bounty on the leader's capital.
        if t.aggression >= 60 && s.tick >= 40 && s.tick % 15 == 3 {
            let leader = (0..n).filter(|o| *o != civ).max_by_key(|o| {
                s.civs[*o as usize]
                    .achievements
                    .tiers
                    .iter()
                    .map(|x| *x as u32)
                    .sum::<u32>()
            });
            let capital = leader
                .and_then(|o| s.civs[o as usize].capital)
                .filter(|c| s.cities.get(*c as usize).is_some_and(|x| x.alive));
            let already = s.contracts.iter().any(|c| {
                c.from == civ
                    && matches!(c.term, ContractTerm::Capture { city } if Some(city) == capital)
            });
            if let (Some(city), false) = (capital, already) {
                wish.push((
                    2,
                    Order::OfferContract {
                        to: None,
                        term: ContractTerm::Capture { city },
                        usdc: (spendable / 3).min(2_000_000),
                        deadline: (s.tick + 40).min(last),
                    },
                ));
                return;
            }
        }
        // Loyal and open: pay a pact partner to keep it.
        if t.loyalty >= 60 && t.openness >= 50 && s.tick % 20 == 11 {
            if let Some(o) = (0..n).find(|o| {
                *o != civ && matches!(s.relation(civ, *o), Relation::Nap { .. }) && !offered_to(*o)
            }) {
                wish.push((
                    3,
                    Order::OfferContract {
                        to: Some(o),
                        term: ContractTerm::KeepNap {
                            every: 5,
                            installments: 4,
                        },
                        usdc: (spendable / 4).min(2_000_000),
                        deadline: (s.tick + 5).min(last),
                    },
                ));
            }
        }
    }
}
