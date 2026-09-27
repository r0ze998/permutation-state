//! Holdings and lazy accrual (design §5.1, §3.5, §6.6).
//!
//! Nothing ticks. Every resource of a holding is an [`Accrual`]
//! `(value, rate, cap, t0)`, settled when something touches it, as in
//! Eternum. Settling is **path independent**: settling at t₁ then t₂ gives
//! exactly the value of settling once at t₂ (the sub-unit remainder is
//! carried, and the clamps are monotone), so when a player or keeper lands
//! a transaction never changes a balance. Rate changes happen at their
//! scheduled times (queue completions, dormancy), never at touch time.
//!
//! Numbers marked [design] are placeholders for the M0 host simulator to
//! tune; the kernels do not depend on their values.

/// Version of this kernel, bound into `RULESET_HASH` (`super::KERNEL_VERSIONS`):
/// bump it whenever an honest outcome changes. v2: M1 CL-02 caps (walls, production, upkeep), CL-03 checked duplicate cost.
pub const HOLDING_VERSION: u16 = 2;

use crate::economy::upkeep_of_effective;
use crate::fixed::{Bps, Milli, MilliTroops, BPS_ONE, MILLI};
use crate::units::{stats, UnitType};
use borsh::{BorshDeserialize, BorshSerialize};

pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;

/// The eight resources (design §5.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Resource {
    Food,
    Wood,
    Stone,
    Ore,
    Horses,
    Gold,
    Science,
    Influence,
}

pub const RESOURCES: usize = 8;

impl Resource {
    pub const ALL: [Resource; RESOURCES] = [
        Resource::Food,
        Resource::Wood,
        Resource::Stone,
        Resource::Ore,
        Resource::Horses,
        Resource::Gold,
        Resource::Science,
        Resource::Influence,
    ];
    /// Science and Influence can only be pledged, never traded or moved.
    pub const fn pledge_only(self) -> bool {
        matches!(self, Resource::Science | Resource::Influence)
    }
}

/// Holding tiers (design §5.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Tier {
    Hamlet,
    Town,
    City,
    Stronghold,
}

impl Tier {
    /// Worked-tile radius: 1, 2, 2, 3.
    pub const fn worked_radius(self) -> u32 {
        match self {
            Tier::Hamlet => 1,
            Tier::Town | Tier::City => 2,
            Tier::Stronghold => 3,
        }
    }
    /// Laurel strength weight of the tier (design §5.4): 1, 1.3, 1.6, 2.0.
    pub const fn weight_bps(self) -> Bps {
        match self {
            Tier::Hamlet => 10_000,
            Tier::Town => 13_000,
            Tier::City => 16_000,
            Tier::Stronghold => 20_000,
        }
    }
    /// Build-queue slots (4 at most) [design].
    pub const fn queue_slots(self) -> usize {
        match self {
            Tier::Hamlet => 2,
            Tier::Town => 3,
            Tier::City | Tier::Stronghold => 4,
        }
    }
    /// Storage cap per resource, milli-units [design].
    pub const fn storage_cap(self) -> Milli {
        match self {
            Tier::Hamlet => 2_000 * MILLI,
            Tier::Town => 6_000 * MILLI,
            Tier::City => 15_000 * MILLI,
            Tier::Stronghold => 40_000 * MILLI,
        }
    }
    pub const fn next(self) -> Option<Tier> {
        match self {
            Tier::Hamlet => Some(Tier::Town),
            Tier::Town => Some(Tier::City),
            Tier::City => Some(Tier::Stronghold),
            Tier::Stronghold => None,
        }
    }
}

/// Holdings per wallet.
pub const MAX_HOLDINGS_PER_WALLET: u8 = 3;
/// Build-queue length.
pub const QUEUE_SLOTS: usize = 4;
/// No owner action for this long → Dormant: production halves, Shield
/// lapses, no laurel emission (design §3.5).
pub const DORMANT_AFTER: i64 = 5 * DAY;
/// A dormant first holding's site is released after this long (§3.4).
pub const RELEASE_AFTER: i64 = 10 * DAY;
/// Production share while dormant.
pub const DORMANT_PRODUCTION_BPS: Bps = 5_000;
/// Shield of a new or re-founded holding (design §6.6) …
pub const SHIELD_SECS: i64 = 48 * HOUR;
/// … and for holdings founded after day 7.
pub const SHIELD_LATE_SECS: i64 = 72 * HOUR;
pub const SHIELD_LATE_FROM_DAY: u32 = 7;

/// Most copies of one building in one holding (CL-03). The simulator's
/// busiest holdings build 12 [sim, 20,000 agents]; 64 leaves 5× headroom
/// and keeps `duplicate_cost` far from overflow for any in-game base.
pub const MAX_DUPLICATES: u32 = 64;

/// Largest wall points a holding can hold (CL-02). The simulator's
/// holdings reach 600 [sim, 20,000 agents]; 2× that. With it a siege needs
/// at most `siege::required_bells(MAX_WALLS, extra)` = 60 + extra bells, so
/// one large Walls item can no longer make a holding immune (first-pass
/// §4.8).
pub const MAX_WALLS: u32 = 1_200;

/// Largest gross production of one resource, milli-units per hour
/// (CL-02): 1,000 units an hour, over 4× the simulator's highest
/// (230 units an hour [sim, 20,000 agents]). `rate × 28 days` stays below
/// 10¹² milli-units, far inside `i64`.
pub const MAX_PRODUCTION_PER_HOUR: i64 = 1_000 * MILLI;

/// Largest upkeep of one resource, milli-units per hour (CL-02): 10⁸
/// units an hour. Troop upkeep is quadratic in troops, so this binds only
/// above ≈ 6 million tier-2 troops on one holding (no economy reaches it;
/// the simulator's highest is 47.3 units an hour); it exists so that
/// every rate and product stays far inside `i64`.
pub const MAX_UPKEEP_PER_HOUR: i64 = 100_000_000 * MILLI;

/// Eternum's quadratic duplicate rule: the n-th copy (n ≥ 1) of a building
/// in one holding costs `base × (1 + 0.5 (n − 1)²)`. Checked (CL-03):
/// `None` for `n > MAX_DUPLICATES` or when the cost does not fit in `u64`.
pub const fn duplicate_cost(base: u64, n: u32) -> Option<u64> {
    if n > MAX_DUPLICATES {
        return None;
    }
    let k = n.saturating_sub(1) as u64;
    // k ≤ 63, so 2 + k² ≤ 3,971: only the product with `base` can overflow.
    match base.checked_mul(2 + k * k) {
        Some(x) => Some(x / 2),
        None => None,
    }
}

/// Shield length of a holding founded on season day `day`.
pub const fn shield_secs(day: u32) -> i64 {
    if day > SHIELD_LATE_FROM_DAY {
        SHIELD_LATE_SECS
    } else {
        SHIELD_SECS
    }
}

/// Troop upkeep of a holding's hosts and garrison in food (and gold) per
/// hour: `economy::upkeep_of_effective` (reused) with troops counted in
/// hundreds, T2 troops ×1.5 [design]. Saturates at `Milli::MAX` for any
/// input (CL-02: no wrap for attacker-shaped entry lists; `set_upkeep`
/// then caps the rate).
pub fn troop_upkeep_per_hour(troops: &[(UnitType, MilliTroops)]) -> Milli {
    const PER: u64 = 100; // troops per upkeep unit
    let eff: u64 = troops
        .iter()
        .filter(|(u, _)| !u.is_civilian())
        .map(|(u, t)| *t as u64 * stats(*u).upkeep_bps as u64 / BPS_ONE as u64)
        .fold(0u64, u64::saturating_add);
    let e = eff / PER;
    // upkeep_of_effective squares e / 1000: keep it inside u64.
    if e / 1000 > 1 << 31 {
        return Milli::MAX;
    }
    i64::try_from(upkeep_of_effective(e))
        .unwrap_or(Milli::MAX)
        .saturating_mul(MILLI)
}

/// One lazily accrued quantity: `value` at `t0`, growing at `rate` per hour
/// up to `cap` (a positive rate never lowers a value already above the cap;
/// a negative rate stops at 0 and reports the shortfall).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Accrual {
    /// Milli-units at `t0`.
    pub value: Milli,
    /// Milli-units per hour (negative: net consumption).
    pub rate: i64,
    pub cap: Milli,
    /// Unix seconds of the last settle.
    pub t0: i64,
    /// Carried remainder of `rate × seconds`, in 0..3600.
    pub frac: i64,
}

impl Accrual {
    pub const fn new(value: Milli, rate: i64, cap: Milli, t0: i64) -> Self {
        Accrual {
            value,
            rate,
            cap,
            t0,
            frac: 0,
        }
    }

    /// Settle to `t` (no-op if `t ≤ t0`). Returns the shortfall: milli-units
    /// a negative rate could not take because the value hit 0.
    pub fn settle(&mut self, t: i64) -> Milli {
        if t <= self.t0 {
            return 0;
        }
        let num = self.frac as i128 + self.rate as i128 * (t - self.t0) as i128;
        let gain = num.div_euclid(HOUR as i128);
        self.frac = num.rem_euclid(HOUR as i128) as i64;
        self.t0 = t;
        let v = self.value as i128 + gain;
        let mut short = 0;
        if self.rate >= 0 {
            if self.value < self.cap {
                self.value = v.min(self.cap as i128) as Milli;
            }
        } else if v < 0 {
            short = (-v).min(i64::MAX as i128) as Milli;
            self.value = 0;
        } else {
            self.value = v as Milli;
        }
        short
    }

    /// The value at `t` without writing.
    pub fn value_at(&self, t: i64) -> Milli {
        let mut a = *self;
        a.settle(t);
        a.value
    }

    /// Settle, then change the rate from `t` on.
    pub fn set_rate(&mut self, t: i64, rate: i64) -> Milli {
        let s = self.settle(t);
        self.rate = rate;
        s
    }

    /// Settle, then add (loot, caravans, refunds: may exceed the cap).
    pub fn add(&mut self, t: i64, amount: Milli) {
        self.settle(t);
        self.value = self.value.saturating_add(amount.max(0));
    }

    /// Settle, then take `amount` if it is there.
    pub fn take(&mut self, t: i64, amount: Milli) -> Result<(), HoldingError> {
        self.settle(t);
        if amount < 0 || self.value < amount {
            return Err(HoldingError::Insufficient);
        }
        self.value -= amount;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldingError {
    Insufficient,
    QueueFull,
    /// Timestamps must not go backwards.
    TimeReversed,
    TopTier,
    /// The effect would take walls, production or upkeep past its cap
    /// (`MAX_WALLS`, `MAX_PRODUCTION_PER_HOUR`, `MAX_UPKEEP_PER_HOUR`),
    /// counting every item already queued (CL-02).
    AboveCap,
}

/// What a finished queue item changes, at its completion time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Effect {
    /// Production of one resource, milli-units per hour.
    Production {
        resource: Resource,
        delta: i64,
    },
    /// Upkeep of one resource, milli-units per hour.
    Upkeep {
        resource: Resource,
        delta: i64,
    },
    /// Next tier: wider worked radius, higher caps, more queue slots.
    TierUp,
    Walls {
        delta: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QueueItem {
    pub done_at: i64,
    pub effect: Effect,
}

/// The rules-side state of a holding (the program stores it zero-copy).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Holding {
    pub tier: Tier,
    /// 1 for a citizen's first holding (occupied, never taken), 2 or 3.
    pub order: u8,
    pub founded_ts: i64,
    pub founded_day: u32,
    pub last_owner_action: i64,
    pub stores: [Accrual; RESOURCES],
    /// Gross production and upkeep, milli-units per hour (≥ 0 each).
    pub production: [i64; RESOURCES],
    pub upkeep: [i64; RESOURCES],
    /// In completion order.
    pub queue: [Option<QueueItem>; QUEUE_SLOTS],
    /// Committed walls (`walls_at` adds finished items not yet committed).
    pub walls: u32,
    /// `commit_walls` has committed every Walls item finished before this.
    pub walls_committed_before: i64,
    /// Starving seconds are not tracked; the food shortfall is summed here
    /// (milli-units) for the troop-attrition rule.
    pub food_shortfall: Milli,
}

impl Holding {
    /// A new holding at `now` (a Hamlet, empty stores).
    pub fn found(now: i64, day: u32, order: u8) -> Holding {
        let cap = Tier::Hamlet.storage_cap();
        Holding {
            tier: Tier::Hamlet,
            order,
            founded_ts: now,
            founded_day: day,
            last_owner_action: now,
            stores: [Accrual::new(0, 0, cap, now); RESOURCES],
            production: [0; RESOURCES],
            upkeep: [0; RESOURCES],
            queue: [None; QUEUE_SLOTS],
            walls: 0,
            walls_committed_before: i64::MIN,
            food_shortfall: 0,
        }
    }

    pub const fn dormant_at(&self) -> i64 {
        self.last_owner_action + DORMANT_AFTER
    }

    pub const fn is_dormant(&self, now: i64) -> bool {
        now >= self.dormant_at()
    }

    /// A dormant first holding's site is released (it becomes a Free City
    /// site; the owner keeps citizenship).
    pub const fn is_released(&self, now: i64) -> bool {
        self.order == 1 && now >= self.last_owner_action + RELEASE_AFTER
    }

    pub const fn shield_until(&self) -> i64 {
        self.founded_ts + shield_secs(self.founded_day)
    }

    /// Shielded (no sieges, its hosts cannot attack) at `now`; dormancy
    /// lapses the Shield.
    pub const fn shielded(&self, now: i64) -> bool {
        now < self.shield_until() && !self.is_dormant(now)
    }

    fn net_rate(&self, r: usize, dormant: bool) -> i64 {
        // Clamped again here (CL-02, second line): a caller that writes the
        // fields directly cannot push a rate out of range.
        let prod = self.production[r].clamp(0, MAX_PRODUCTION_PER_HOUR);
        let p = if dormant {
            prod * DORMANT_PRODUCTION_BPS as i64 / BPS_ONE as i64
        } else {
            prod
        };
        p - self.upkeep[r].clamp(0, MAX_UPKEEP_PER_HOUR)
    }

    fn settle_stores(&mut self, t: i64) {
        for (i, s) in self.stores.iter_mut().enumerate() {
            let short = s.settle(t);
            if i == Resource::Food as usize {
                self.food_shortfall = self.food_shortfall.saturating_add(short);
            }
        }
    }

    fn apply_rates(&mut self, t: i64) {
        let dormant = self.is_dormant(t);
        for r in 0..RESOURCES {
            let rate = self.net_rate(r, dormant);
            self.stores[r].set_rate(t, rate);
        }
    }

    fn apply(&mut self, t: i64, e: Effect) {
        // `enqueue` refuses effects past the caps; `apply` clamps as the
        // second line (CL-02).
        match e {
            Effect::Production { resource, delta } => {
                let p = &mut self.production[resource as usize];
                *p = p.saturating_add(delta).clamp(0, MAX_PRODUCTION_PER_HOUR);
            }
            Effect::Upkeep { resource, delta } => {
                let u = &mut self.upkeep[resource as usize];
                *u = u.saturating_add(delta).clamp(0, MAX_UPKEEP_PER_HOUR);
            }
            Effect::TierUp => {
                if let Some(n) = self.tier.next() {
                    self.tier = n;
                    for s in self.stores.iter_mut() {
                        s.settle(t);
                        s.cap = n.storage_cap();
                    }
                }
            }
            Effect::Walls { delta } => self.walls = self.walls.saturating_add(delta).min(MAX_WALLS),
        }
    }

    /// What `field` would reach if every queued item and then `e` finished:
    /// the value the caps are checked against (positive deltas summed,
    /// negative ones ignored, so the order of completion cannot matter).
    fn projected_after(&self, e: Effect) -> Result<(), HoldingError> {
        let pos = |d: i64| d.max(0) as i128;
        let (mut prod, mut upk) = ([0i128; RESOURCES], [0i128; RESOURCES]);
        for r in 0..RESOURCES {
            prod[r] = self.production[r].max(0) as i128;
            upk[r] = self.upkeep[r].max(0) as i128;
        }
        let mut walls = self.walls as i128;
        let items = self.queue.iter().flatten().map(|q| q.effect).chain([e]);
        for x in items {
            match x {
                Effect::Production { resource, delta } => prod[resource as usize] += pos(delta),
                Effect::Upkeep { resource, delta } => upk[resource as usize] += pos(delta),
                Effect::Walls { delta } => walls += delta as i128,
                Effect::TierUp => {}
            }
        }
        let over = walls > MAX_WALLS as i128
            || prod.iter().any(|p| *p > MAX_PRODUCTION_PER_HOUR as i128)
            || upk.iter().any(|u| *u > MAX_UPKEEP_PER_HOUR as i128);
        // A negative delta larger than any cap is garbage, not a demolition.
        let garbage = match e {
            Effect::Production { delta, .. } => delta < -MAX_PRODUCTION_PER_HOUR,
            Effect::Upkeep { delta, .. } => delta < -MAX_UPKEEP_PER_HOUR,
            _ => false,
        };
        if over || garbage {
            Err(HoldingError::AboveCap)
        } else {
            Ok(())
        }
    }

    /// Walls as the clash of a bell starting at `bell_start_ts` reads them:
    /// the committed walls plus every finished Walls item that completed
    /// **before** the bell started (a completion during bell b counts from
    /// b + 1, like any change, design §6.3). A pure function of the queue,
    /// so a clash resolved late reads the same walls as one resolved at
    /// once. Refused for a bell before the last commit.
    pub fn walls_at(&self, bell_start_ts: i64) -> Result<u32, HoldingError> {
        if bell_start_ts < self.walls_committed_before {
            return Err(HoldingError::TimeReversed);
        }
        let mut w = self.walls;
        for q in self.queue.iter().flatten() {
            if let Effect::Walls { delta } = q.effect {
                if q.done_at < bell_start_ts {
                    w = w.saturating_add(delta);
                }
            }
        }
        Ok(w.min(MAX_WALLS))
    }

    /// Move finished Walls items into `walls` once no clash can read a bell
    /// starting before `before_ts` any more: the program passes the start
    /// of the first bell the holding's province has not resolved. Until
    /// then a finished Walls item keeps its queue slot (lag only waits).
    pub fn commit_walls(&mut self, before_ts: i64) {
        if before_ts <= self.walls_committed_before {
            return;
        }
        for q in self.queue.iter_mut() {
            if let Some(QueueItem {
                done_at,
                effect: Effect::Walls { delta },
            }) = *q
            {
                if done_at < before_ts {
                    self.walls = self.walls.saturating_add(delta).min(MAX_WALLS);
                    *q = None;
                }
            }
        }
        self.walls_committed_before = before_ts;
    }

    /// Settle every store to `now`, applying queue completions and the
    /// start of dormancy at their own times, in time order (ties: queue
    /// order, then dormancy). Path independent: any sequence of settles
    /// ending at `now` gives the same holding.
    pub fn settle(&mut self, now: i64) -> Result<(), HoldingError> {
        let t0 = self.stores[0].t0;
        if now < t0 {
            return Err(HoldingError::TimeReversed);
        }
        loop {
            let t = self.stores[0].t0;
            let mut next: Option<(i64, usize)> = None;
            for (i, q) in self.queue.iter().enumerate() {
                if let Some(q) = q {
                    // Walls do not change rates; they are committed by
                    // `commit_walls` once the province's clashes have
                    // read them (`walls_at`).
                    if matches!(q.effect, Effect::Walls { .. }) {
                        continue;
                    }
                    if q.done_at <= now && next.is_none_or(|(nt, _)| q.done_at < nt) {
                        next = Some((q.done_at.max(t), i));
                    }
                }
            }
            let dorm = self.dormant_at();
            let dorm_due = dorm > t && dorm <= now;
            match next {
                Some((qt, i)) if !dorm_due || qt <= dorm => {
                    self.settle_stores(qt);
                    let item = self.queue[i].take();
                    if let Some(item) = item {
                        self.apply(qt, item.effect);
                    }
                    self.apply_rates(qt);
                }
                _ if dorm_due => {
                    self.settle_stores(dorm);
                    self.apply_rates(dorm);
                }
                _ => break,
            }
        }
        self.settle_stores(now);
        Ok(())
    }

    /// An owner (or delegate) action at `now`: settle, then the holding is
    /// active again (full production from `now`).
    pub fn touch_owner(&mut self, now: i64) -> Result<(), HoldingError> {
        self.settle(now)?;
        self.last_owner_action = now;
        self.apply_rates(now);
        Ok(())
    }

    /// Queue `effect` to finish `duration` seconds after the last queued
    /// item (or `now`). Settles first. Refused (`AboveCap`, CL-02) if the
    /// effect, on top of the current values and every queued item, would
    /// take walls, a production or an upkeep past its cap.
    pub fn enqueue(
        &mut self,
        now: i64,
        duration: i64,
        effect: Effect,
    ) -> Result<i64, HoldingError> {
        self.settle(now)?;
        let used = self.queue.iter().filter(|q| q.is_some()).count();
        if used >= self.tier.queue_slots() {
            return Err(HoldingError::QueueFull);
        }
        if matches!(effect, Effect::TierUp) && self.tier.next().is_none() {
            return Err(HoldingError::TopTier);
        }
        self.projected_after(effect)?;
        let start = self
            .queue
            .iter()
            .flatten()
            .map(|q| q.done_at)
            .max()
            .unwrap_or(now)
            .max(now);
        let done_at = start.saturating_add(duration.max(0));
        let slot = self.queue.iter().position(|q| q.is_none());
        match slot {
            Some(s) => {
                self.queue[s] = Some(QueueItem { done_at, effect });
                Ok(done_at)
            }
            None => Err(HoldingError::QueueFull),
        }
    }

    /// Settle, then pay `cost` (one amount per resource) or nothing.
    pub fn pay(&mut self, now: i64, cost: &[Milli; RESOURCES]) -> Result<(), HoldingError> {
        self.settle(now)?;
        if (0..RESOURCES).any(|r| cost[r] < 0 || self.stores[r].value < cost[r]) {
            return Err(HoldingError::Insufficient);
        }
        for (s, c) in self.stores.iter_mut().zip(cost) {
            s.value -= c;
        }
        Ok(())
    }

    /// Settle, then add to one store (loot, caravans, refunds). Use this,
    /// not `Accrual::add`, so every store keeps the same `t0`.
    pub fn credit(&mut self, now: i64, r: Resource, amount: Milli) -> Result<(), HoldingError> {
        self.settle(now)?;
        self.stores[r as usize].add(now, amount);
        Ok(())
    }

    /// Current stock of every resource (settled view, no write).
    pub fn stock_at(&self, now: i64) -> [Milli; RESOURCES] {
        let mut h = self.clone();
        let _ = h.settle(now);
        core::array::from_fn(|r| h.stores[r].value)
    }

    /// Replace the upkeep of one resource from `now` (troop changes).
    pub fn set_upkeep(
        &mut self,
        now: i64,
        resource: Resource,
        per_hour: i64,
    ) -> Result<(), HoldingError> {
        self.settle(now)?;
        self.upkeep[resource as usize] = per_hour.clamp(0, MAX_UPKEEP_PER_HOUR);
        self.apply_rates(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_rule() {
        assert_eq!(duplicate_cost(100, 1), Some(100));
        assert_eq!(duplicate_cost(100, 2), Some(150));
        assert_eq!(duplicate_cost(100, 3), Some(300));
        assert_eq!(duplicate_cost(100, 0), Some(100));
    }

    #[test]
    fn accrual_is_path_independent() {
        for rate in [-7_001i64, -1, 0, 1, 333, 12_345] {
            let a0 = Accrual::new(5_000, rate, 20_000, 0);
            let mut once = a0;
            let s_once = once.settle(40_000);
            let mut steps = a0;
            let mut s_steps = 0;
            for t in [1, 7, 3_599, 3_600, 10_001, 39_999, 40_000] {
                s_steps += steps.settle(t);
            }
            assert_eq!(once, steps, "rate {rate}");
            assert_eq!(s_once, s_steps, "rate {rate}");
        }
    }
}
