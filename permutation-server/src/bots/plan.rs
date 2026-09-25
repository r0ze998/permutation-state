//! One tick's orders: each step adds wishes `(priority, order)` in a fixed
//! order, and `within_budget` keeps the most important that fit each
//! office's budget, one per unit. The step order is part of the behaviour
//! (a stable sort breaks priority ties by it): do not reorder the steps.

use permutation_rules::buildings::Building;
use permutation_rules::gov::Role;
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side, StandingOrder, StandingTarget};
use permutation_rules::state::{
    Civ, CivId, Owner, ProposalKind, QueueItem, Relation, StandingRule, UnitId, WorldState,
};
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::units::{stats, UnitClass, UnitType};
use permutation_rules::Ruleset;
use std::collections::BTreeSet;

use super::geo::{city_count, good_site, my_units, path_to, site_value, troops_of};
use super::{Bot, Persona};

/// (priority, order): lower number = more important.
pub(super) type Wishes = Vec<(u8, Order)>;

/// What every step reads: the world, the rules and this nation.
pub(super) struct Ctx<'a> {
    pub s: &'a WorldState,
    pub r: &'a Ruleset,
    pub civ: CivId,
    pub me: &'a Civ,
    pub n: CivId,
    /// This nation's capital hex, if it has a capital.
    pub my_cap: Option<Hex>,
}

impl<'a> Ctx<'a> {
    fn new(s: &'a WorldState, r: &'a Ruleset, civ: CivId) -> Self {
        let me = &s.civs[civ as usize];
        Ctx {
            s,
            r,
            civ,
            me,
            n: s.civs.len() as CivId,
            my_cap: me.capital.map(|c| s.cities[c as usize].hex),
        }
    }

    /// Treaty partners (NAP or alliance).
    fn partners(&self) -> usize {
        (0..self.n)
            .filter(|o| {
                *o != self.civ
                    && matches!(
                        self.s.relation(self.civ, *o),
                        Relation::Nap { .. } | Relation::Alliance { .. }
                    )
            })
            .count()
    }

    /// Distance between the two capitals (theirs living).
    fn cap_distance(&self, o: CivId) -> Option<u32> {
        let s = self.s;
        let mine = s.cities.get(self.me.capital? as usize)?.hex;
        let theirs = s
            .cities
            .get(s.civs[o as usize].capital? as usize)
            .filter(|c| c.alive)?;
        Some(theirs.hex.distance(mine))
    }

    /// The warlord's next target is the nearest civ at peace; any other civ
    /// at peace is a pact candidate.
    fn not_target(&self, o: CivId) -> bool {
        let Some(d) = self.cap_distance(o) else {
            return false;
        };
        (0..self.n).any(|x| {
            x != self.civ
                && x != o
                && matches!(self.s.relation(self.civ, x), Relation::Peace)
                && self.cap_distance(x).is_some_and(|dx| dx < d)
        })
    }

    /// The civ with the nearest living capital among those `filter` keeps.
    fn nearest_civ(&self, filter: &dyn Fn(CivId) -> bool) -> Option<CivId> {
        let cap = self.my_cap?;
        (0..self.n)
            .filter(|o| *o != self.civ && filter(*o))
            .filter_map(|o| {
                let oc = self.s.civs[o as usize].capital?;
                let c = self.s.cities.get(oc as usize).filter(|c| c.alive)?;
                Some((c.hex.distance(cap), o))
            })
            .min()
            .map(|(_, o)| o)
    }

    /// Whether this civ has an open proposal to `o`.
    fn offered(&self, o: CivId) -> bool {
        self.s
            .proposals
            .iter()
            .any(|p| p.from == self.civ && p.to == o)
    }
}

impl Bot {
    /// Orders for this tick, from the state `s` every player sees (perfect
    /// information). `explored` marks the tiles the nation has looked at;
    /// scouts walk toward the others.
    pub fn orders(&mut self, s: &WorldState, r: &Ruleset, explored: &[bool]) -> Vec<Order> {
        let cx = Ctx::new(s, r, self.civ);
        let mut wish = Wishes::new();
        self.answer_proposals(&cx, &mut wish);
        self.diplomacy(&cx, &mut wish);
        self.contracts(s, r, &mut wish);
        self.envoys(&cx, &mut wish);
        self.trade(&cx, &mut wish);
        self.research(&cx, &mut wish);
        self.city_queues(&cx, &mut wish);
        self.settlers(&cx, &mut wish);
        self.scouts(&cx, explored, &mut wish);
        let garrison = self.armies(&cx, &mut wish);
        self.standing_rules(&cx, garrison, &mut wish);
        within_budget(&cx, wish)
    }

    /// Accept the proposals addressed to this nation that suit it.
    fn answer_proposals(&self, cx: &Ctx, wish: &mut Wishes) {
        let (s, r, civ, me) = (cx.s, cx.r, cx.civ, cx.me);
        let partners = cx.partners();
        for p in s
            .proposals
            .iter()
            .filter(|p| p.to == civ && p.tick < s.tick)
        {
            let accept = match (p.kind, self.persona) {
                (ProposalKind::Peace, Persona::Warlord) => troops_of(s, civ) < troops_of(s, p.from),
                (ProposalKind::Peace, _) => true,
                (ProposalKind::Nap { .. }, Persona::Warlord) => {
                    partners == 0 && me.gold >= 40_000 && cx.not_target(p.from)
                }
                (ProposalKind::Nap { .. }, _) => me.gold >= 40_000,
                (ProposalKind::Alliance, Persona::Diplomat | Persona::Builder) => true,
                (ProposalKind::Alliance, _) => false,
            };
            if accept {
                let o = match p.kind {
                    ProposalKind::Peace => Order::AcceptPeace { civ: p.from },
                    ProposalKind::Nap { .. } => Order::AcceptNap {
                        civ: p.from,
                        bond: r.nap_min_bond,
                    },
                    ProposalKind::Alliance => Order::AcceptAlliance { civ: p.from },
                };
                wish.push((0, o));
            }
        }
    }

    /// This nation's own initiatives: war and peace, pacts, alliances.
    fn diplomacy(&mut self, cx: &Ctx, wish: &mut Wishes) {
        let (s, r, civ, me, n) = (cx.s, cx.r, cx.civ, cx.me, cx.n);
        let partners = cx.partners();
        match self.persona {
            Persona::Warlord => {
                let at_war = (0..n).any(|o| s.at_war(civ, o));
                if !at_war && s.tick >= 30 && troops_of(s, civ) >= 12 {
                    if let Some(t) =
                        cx.nearest_civ(&|o| matches!(s.relation(civ, o), Relation::Peace))
                    {
                        wish.push((1, Order::DeclareWar { civ: t }));
                        self.war_started.insert(t, s.tick);
                    }
                }
                for o in 0..n {
                    let since = self.war_started.get(&o).copied().unwrap_or(s.tick);
                    // Holding a city taken from them: make peace and keep it.
                    let holds_theirs = s
                        .cities
                        .iter()
                        .any(|c| c.alive && c.owner == Some(civ) && c.captured_from == Some(o));
                    let long = s.tick - since >= if holds_theirs { 20 } else { 45 };
                    if s.at_war(civ, o) && (long || troops_of(s, civ) < 4) {
                        wish.push((1, Order::ProposePeace { civ: o }));
                    }
                }
                // One pact with the farthest civ at peace, to keep a treaty
                // partner while fighting the nearest.
                if s.tick % 5 == 0 && partners == 0 && me.gold >= 50_000 {
                    if let Some(t) = (0..n)
                        .filter(|o| {
                            *o != civ
                                && matches!(s.relation(civ, *o), Relation::Peace)
                                && !cx.offered(*o)
                                && cx.not_target(*o)
                        })
                        .filter_map(|o| Some((cx.cap_distance(o)?, o)))
                        .max()
                        .map(|(_, o)| o)
                    {
                        wish.push((
                            2,
                            Order::ProposeNap {
                                civ: t,
                                bond: r.nap_min_bond,
                            },
                        ));
                    }
                }
            }
            Persona::Diplomat | Persona::Builder | Persona::Scholar => {
                // Keep up to three treaty partners: offer non-aggression pacts to
                // the nearest civs at peace, and (diplomats) turn pacts into alliances.
                if s.tick % 5 == 0 && partners < 3 && me.gold >= 50_000 {
                    if let Some(t) = cx.nearest_civ(&|o| {
                        matches!(s.relation(civ, o), Relation::Peace) && !cx.offered(o)
                    }) {
                        wish.push((
                            2,
                            Order::ProposeNap {
                                civ: t,
                                bond: r.nap_min_bond,
                            },
                        ));
                    }
                }
                if self.persona == Persona::Diplomat && s.tick % 10 == 5 {
                    if let Some(t) = cx.nearest_civ(&|o| {
                        matches!(s.relation(civ, o), Relation::Nap { .. }) && !cx.offered(o)
                    }) {
                        wish.push((1, Order::ProposeAlliance { civ: t }));
                    }
                }
            }
        }
        for o in 0..n {
            if s.at_war(civ, o) && self.persona != Persona::Warlord {
                wish.push((1, Order::ProposePeace { civ: o }));
            }
        }
    }

    /// Every persona courts a city-state, diplomats earliest: the free one
    /// where this civ is closest to (or already is) suzerain, then the nearest.
    fn envoys(&self, cx: &Ctx, wish: &mut Wishes) {
        let (s, civ, me) = (cx.s, cx.civ, cx.me);
        let envoy_at = match self.persona {
            Persona::Diplomat => 15_000,
            _ => 30_000,
        };
        if me.influence < envoy_at {
            return;
        }
        let Some(cap) = cx.my_cap else { return };
        if let Some(cs) = s
            .city_states
            .iter()
            .filter(|cs| cs.captured_by.is_none() && cs.suzerain.is_none_or(|z| z == civ))
            .min_by_key(|cs| {
                (
                    std::cmp::Reverse(cs.influence[civ as usize]),
                    cs.hex.distance(cap),
                )
            })
        {
            wish.push((
                3,
                Order::SendEnvoy {
                    city_state: cs.id,
                    influence: (me.influence / 1000) as u32,
                },
            ));
        }
    }

    /// Concord through trade: the gold market for surplus or missing iron
    /// and horses, and small gold gifts to treaty partners and allies.
    fn trade(&self, cx: &Ctx, wish: &mut Wishes) {
        let (s, civ, me, n) = (cx.s, cx.civ, cx.me, cx.n);
        if me.techs.has(Tech::Currency) && s.tick % 5 == 2 {
            for (good, stock) in [(Good::Iron, me.iron), (Good::Horses, me.horses)] {
                let whole = (stock / 1000) as u32;
                if whole > 20 {
                    wish.push((
                        4,
                        Order::MarketTrade {
                            good,
                            side: Side::Sell,
                            amount: whole - 10,
                            limit_gold: 0,
                        },
                    ));
                } else if whole < 5 && me.gold > 150_000 {
                    wish.push((
                        4,
                        Order::MarketTrade {
                            good,
                            side: Side::Buy,
                            amount: 5,
                            limit_gold: (me.gold / 1000 / 3) as u32,
                        },
                    ));
                }
            }
        }
        if s.tick % 10 == 7 && me.gold > 100_000 {
            let friends: Vec<CivId> = (0..n)
                .filter(|o| {
                    *o != civ
                        && matches!(
                            s.relation(civ, *o),
                            Relation::Nap { .. } | Relation::Alliance { .. }
                        )
                })
                .collect();
            let gift = (me.last.gold / 2).max(1);
            for o in friends.into_iter().take(2) {
                wish.push((
                    4,
                    Order::Transfer {
                        civ: o,
                        good: Good::Gold,
                        amount: gift,
                    },
                ));
            }
        }
    }

    /// Keep three techs queued, the persona's order first.
    fn research(&self, cx: &Ctx, wish: &mut Wishes) {
        let me = cx.me;
        if me.research_queue.len() >= 3 {
            return;
        }
        let mut plan = me.techs;
        for t in &me.research_queue {
            plan.insert(*t);
        }
        let mut q = me.research_queue.clone();
        let order = self
            .persona
            .research()
            .iter()
            .chain(TECHS.iter().map(|t| &t.tech));
        for t in order {
            if q.len() == 3 {
                break;
            }
            if !plan.has(*t) && plan.prereqs_met(*t) {
                plan.insert(*t);
                q.push(*t);
            }
        }
        if q.len() > me.research_queue.len() {
            wish.push((2, Order::SetResearch { techs: q }));
        }
    }

    /// Re-plan the queue of every city that needs it (Star Gate, settlers,
    /// buildings, troops), and buy production in the capital when rich.
    fn city_queues(&self, cx: &Ctx, wish: &mut Wishes) {
        let (s, civ, me) = (cx.s, cx.civ, cx.me);
        let settlers_alive = my_units(s, civ)
            .filter(|u| u.unit_type == UnitType::Settler)
            .count();
        let queued_settlers = s
            .living_cities_of(civ)
            .filter(|c| c.queue.first() == Some(&QueueItem::Settler))
            .count();
        let mut want_settlers = self
            .persona
            .target_cities()
            .saturating_sub(city_count(s, civ) + settlers_alive + queued_settlers);
        let troop_cap = if self.persona == Persona::Warlord {
            40
        } else {
            16
        };
        let army = troops_of(s, civ);
        let has = |t: Tech| me.techs.has(t);
        for c in s.living_cities_of(civ) {
            let buildable: Vec<Building> = self
                .persona
                .buildings()
                .iter()
                .copied()
                .filter(|b| {
                    !c.buildings.has(*b)
                        && permutation_rules::buildings::info(*b).tech.is_none_or(has)
                        && !c.queue.contains(&QueueItem::Building(*b))
                })
                .collect();
            let star = if matches!(self.persona, Persona::Scholar | Persona::Builder)
                && me.capital == Some(c.id)
            {
                let stage = c.buildings.star_gate_stages() as usize;
                [
                    (Building::StarGate1, Tech::Astronomy),
                    (Building::StarGate2, Tech::Physics),
                    (Building::StarGate3, Tech::CelestialMechanics),
                ]
                .get(stage)
                .filter(|(b, t)| has(*t) && !c.queue.contains(&QueueItem::Building(*b)))
                .map(|(b, _)| *b)
            } else {
                None
            };
            let settle = want_settlers > 0 && c.pop >= 2;
            // CityQueueRepeat keeps re-queuing the last unit, so a queue that
            // only repeats a unit is re-planned whenever something better exists.
            let repeating = c.queue.len() == 1
                && matches!(c.queue[0], QueueItem::Troops { .. } | QueueItem::Settler);
            let needs = c.queue.is_empty()
                || (repeating
                    && (settle
                        || star.is_some()
                        || !buildable.is_empty()
                        || army >= troop_cap
                        || (c.queue[0] == QueueItem::Settler && want_settlers == 0)));
            if !needs {
                continue;
            }
            let mut items = Vec::new();
            if let Some(b) = star {
                items.push(QueueItem::Building(b));
            }
            if settle {
                items.push(QueueItem::Settler);
                want_settlers -= 1;
            }
            for b in &buildable {
                if items.len() >= 2 {
                    break;
                }
                items.push(QueueItem::Building(*b));
            }
            if army < troop_cap && items.len() < 3 {
                let unit = if has(Tech::IronWorking) && me.iron >= 5_000 {
                    UnitType::Pikeman
                } else if self.persona == Persona::Warlord
                    && has(Tech::HorsebackRiding)
                    && s.tick % 2 == 0
                {
                    UnitType::Horseman
                } else if has(Tech::Archery) && s.tick % 3 == 0 {
                    UnitType::Archer
                } else {
                    UnitType::Spearman
                };
                let n = if self.persona == Persona::Warlord {
                    8
                } else {
                    5
                };
                items.push(QueueItem::Troops { unit, n });
            }
            items.truncate(3);
            if items != c.queue {
                wish.push((2, Order::SetQueue { city: c.id, items }));
            }
        }
        if matches!(self.persona, Persona::Builder | Persona::Scholar) && me.gold > 150_000 {
            if let Some(cap) = me.capital {
                wish.push((
                    4,
                    Order::Purchase {
                        city: cap,
                        gold: 60,
                    },
                ));
            }
        }
    }

    /// Settlers found where they stand if it is a good site near home, or
    /// walk to the best unclaimed site 3–6 hexes from the nation's cities.
    fn settlers(&self, cx: &Ctx, wish: &mut Wishes) {
        let (s, r, civ) = (cx.s, cx.r, cx.civ);
        let mut claimed: Vec<Hex> = Vec::new();
        for u in my_units(s, civ).filter(|u| u.unit_type == UnitType::Settler) {
            if good_site(s, r, civ, u.hex)
                && s.living_cities_of(civ).all(|c| c.hex.distance(u.hex) <= 7)
            {
                wish.push((1, Order::FoundCity { settler: u.id }));
                continue;
            }
            if !u.path.is_empty() {
                continue;
            }
            let target = s
                .map
                .tiles
                .iter()
                .filter(|t| {
                    let near_mine = s
                        .living_cities_of(civ)
                        .map(|c| c.hex.distance(t.hex))
                        .min()
                        .unwrap_or(99);
                    (3..=6).contains(&near_mine)
                        && !claimed.iter().any(|c| c.distance(t.hex) < 3)
                        && good_site(s, r, civ, t.hex)
                })
                .max_by_key(|t| {
                    // Ties by a per-season random key, not the tile index:
                    // index order depends on direction, which would favour
                    // some positions of a symmetric map over others.
                    (
                        site_value(s, t.hex),
                        std::cmp::Reverse(t.hex.distance(u.hex)),
                        permutation_rules::rng::rand_id(
                            &s.season_seed,
                            b"bot-site",
                            ((t.hex.q as i64 as u64) << 32)
                                ^ (t.hex.r as i64 as u64 & 0xffff_ffff)
                                ^ ((civ as u64) << 48),
                        ),
                    )
                })
                .map(|t| t.hex);
            if let Some(goal) = target {
                claimed.push(goal);
                if let Some(path) = path_to(s, r, civ, u.hex, |h| h == goal) {
                    wish.push((2, Order::MoveUnit { unit: u.id, path }));
                }
            }
        }
    }

    /// Scouts walk to the nearest tile not yet explored.
    fn scouts(&self, cx: &Ctx, explored: &[bool], wish: &mut Wishes) {
        let (s, r, civ) = (cx.s, cx.r, cx.civ);
        for u in my_units(s, civ).filter(|u| u.unit_type == UnitType::Scout && u.path.is_empty()) {
            let unexplored = |h: Hex| s.map.index_of(h).is_some_and(|i| !explored[i]);
            if let Some(path) = path_to(s, r, civ, u.hex, unexplored) {
                wish.push((3, Order::MoveUnit { unit: u.id, path }));
            }
        }
    }

    /// Armies attack anything hostile in reach (cities first, then the
    /// weakest army), else march on the nearest enemy city (`targets`
    /// first). Returns the capital's garrison, which stays home.
    fn armies(&self, cx: &Ctx, wish: &mut Wishes) -> Option<UnitId> {
        let (s, r, civ, n) = (cx.s, cx.r, cx.civ, cx.n);
        let enemies: Vec<CivId> = (0..n).filter(|o| s.at_war(civ, *o)).collect();
        let garrison = cx.my_cap.and_then(|cap| {
            my_units(s, civ)
                .find(|u| u.hex == cap && !u.unit_type.is_civilian())
                .map(|u| u.id)
        });
        for u in my_units(s, civ).filter(|u| !u.unit_type.is_civilian()) {
            let st = stats(u.unit_type);
            let reach = if st.class == UnitClass::Ranged {
                st.range as u32
            } else {
                1
            };
            let enemy_city = s.cities.iter().find(|c| {
                c.alive
                    && c.owner.is_some_and(|o| enemies.contains(&o))
                    && c.hex.distance(u.hex) <= reach
            });
            if let Some(c) = enemy_city {
                wish.push((
                    1,
                    Order::Attack {
                        army: u.id,
                        target: AttackTarget::City(c.id),
                    },
                ));
                continue;
            }
            let enemy_unit = s
                .units
                .iter()
                .filter(|e| {
                    e.alive
                        && matches!(e.owner, Owner::Civ(o) if enemies.contains(&o))
                        && e.hex.distance(u.hex) <= reach
                })
                .min_by_key(|e| e.troops);
            if let Some(e) = enemy_unit {
                wish.push((
                    1,
                    Order::Attack {
                        army: u.id,
                        target: AttackTarget::Unit(e.id),
                    },
                ));
                continue;
            }
            if Some(u.id) == garrison || enemies.is_empty() || !u.path.is_empty() {
                continue;
            }
            let target = s
                .cities
                .iter()
                .filter(|c| c.alive && c.owner.is_some_and(|o| enemies.contains(&o)))
                .min_by_key(|c| (!self.targets.contains(&c.id), c.hex.distance(u.hex)))
                .map(|c| c.hex);
            if let Some(goal) = target {
                if let Some(path) = path_to(s, r, civ, u.hex, |h| h.distance(goal) == 1) {
                    wish.push((3, Order::MoveUnit { unit: u.id, path }));
                }
            }
        }
        garrison
    }

    /// Standing rules (§13), set once and then run for free every tick: the
    /// garrison defends, other armies (not a warlord's) retreat when
    /// outmatched; rich builders and scholars auto-purchase in the capital.
    fn standing_rules(&self, cx: &Ctx, garrison: Option<UnitId>, wish: &mut Wishes) {
        let (s, civ, me) = (cx.s, cx.civ, cx.me);
        for u in my_units(s, civ)
            .filter(|u| !u.unit_type.is_civilian() && u.standing == StandingRule::None)
        {
            let rule = if Some(u.id) == garrison {
                StandingOrder::AutoDefend { radius: 2 }
            } else if self.persona != Persona::Warlord {
                StandingOrder::Retreat { ratio_bps: 15_000 }
            } else {
                continue;
            };
            wish.push((
                4,
                Order::SetStanding {
                    target: StandingTarget::Unit(u.id),
                    rule,
                },
            ));
        }
        if matches!(self.persona, Persona::Builder | Persona::Scholar) && me.gold > 120_000 {
            if let Some(cap) = me
                .capital
                .filter(|c| s.cities[*c as usize].standing.auto_purchase == 0)
            {
                wish.push((
                    4,
                    Order::SetStanding {
                        target: StandingTarget::City(cap),
                        rule: StandingOrder::AutoPurchase { max_gold: 30 },
                    },
                ));
            }
        }
    }
}

/// Keep the most important wishes that fit each office's budget (V5 §5.2),
/// one order per unit.
fn within_budget(cx: &Ctx, mut wish: Wishes) -> Vec<Order> {
    let (s, r, civ) = (cx.s, cx.r, cx.civ);
    let budget: [usize; 4] = core::array::from_fn(|i| {
        permutation_rules::orders::spendable(s, r, civ, Role::ALL[i]) as usize
    });
    wish.sort_by_key(|(p, _)| *p);
    let mut used_units = BTreeSet::new();
    let mut out = Vec::new();
    let mut cost = [0usize; 4];
    for (_, o) in wish {
        if let Some(u) = o.commanded_unit() {
            if !used_units.insert(u) {
                continue;
            }
        }
        let Some(role) = Role::ALL
            .into_iter()
            .find(|x| permutation_rules::orders::role_allows(s, *x, &o))
        else {
            continue;
        };
        let c = o.cost() as usize;
        if cost[role.index()] + c > budget[role.index()] {
            continue;
        }
        cost[role.index()] += c;
        out.push(o);
    }
    out
}
