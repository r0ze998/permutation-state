//! The march season script (see the module note of [`super`]).
//!
//! One beacon region `r` (the one with the most ring-2/3 provinces), five
//! provinces A–E of it, ten citizens (factions 0, 1, 2) with a holding each
//! from one ticket cohort, and these marches (arrival bells 8 and 9):
//!
//! | host | from → to | seal | what happens | outcome |
//! |---|---|---|---|---|
//! | h0 (f0, 800) | A → B @8 | valid | keeper reveals in window; fights B's resident h2 | its fate |
//! | h1 (f0, 300) | A → B @8 | valid, tip = `tip_min` | never revealed (keeper B off) | routed (valid seal unrevealed: warning) |
//! | h3 (f1, 500) | B → A @8 | garbage (valid commitment) | the owner reveals in-bell; B resolves bell 6 late (lagging origin) | bad seal, code 1, destroyed |
//! | w1–w4 (f2) | C/D/E → A @8 | valid | fill the four slots | their fates |
//! | w5 (f2, 1000) | E → A @8 | valid | revealed late and priced: displaces w4; its evidence is claimed (ClaimDefence) | its fate |
//! | w6 (f2, 600) | E → A @8 | valid | its Reveal is refused (`QuotaRefused`, a failed transaction) | bounced-unranked, no loss |
//! | h9 (f0, 200) | A → B @9 | garbage | never revealed; settled **after the bell is archived** | bad seal |
//! | h10 (f1, 250) | B → A @9 | valid seal of an invalid plaintext | never revealed | bad seal, code 5 |
//!
//! Also: SkipQuiet runs in every province, an explore settled from the seed
//! of its bell, archived anchors (ArchiveAnchors after `archive_after`), the
//! closed anchors, caches and settled slots.

use std::collections::{BTreeMap, BTreeSet};

use fclient::ix::{
    self, AnchorSource, ArchiveItem, ClaimSlot, DepartArgs, HoldingRef, Player, RevealArgs,
    SeedSource, SettleTransitArgs, Site,
};
use fclient::seal as fseal;
use fclient::tx::TxBudget;
use fclient::{Address, Keypair, Signer};
use frontier_abi::addr::{day_of, holding_key_of_host, host_id};
use frontier_abi::entry::{read_entry, write_entry, Entry, EntryOp};
use frontier_abi::layout::{
    beacon::{
        anchor_archive as AA, archive_entry as AE, bell_anchor as BA, defence_claim as DC,
        seed_cache as SC,
    },
    clash::{arrival as AR, arrival_day as AD, arrival_slot as AS, clash_inputs as CI},
    player::{citizen as C, explore as X, holding as H, transit as T},
    province::{camp as CP, cohort as CO, entry as E, province as PV, site as SM},
    world::{
        beacon_log as BL, frontier as FR, join_shard as JS, province_fund as PF, ring_seed as RS,
        season as S,
    },
    AccountKind,
};
use frontier_abi::log::{self as plog, settle_outcome, transit_outcome, EntityKind as En, Kind};
use permutation_rules::frontier::beacon;
use permutation_rules::frontier::camp;
use permutation_rules::frontier::clash::{
    admit_arrival, apply_slot, ready_bell_after, ClashOutcome, FactionSlots, Fate, SlotDecision,
    SlotEntry,
};
use permutation_rules::frontier::explore;
use permutation_rules::frontier::fees::{self, DefenceParams};
use permutation_rules::frontier::geometry::{region_of, ring_provinces, ProvinceCoord};
use permutation_rules::frontier::host::{rout_survivors, Host, Stamina};
use permutation_rules::frontier::terrain::generate_province;
use permutation_rules::hash::sha256;

use super::{Gen, OP};
use crate::checks::v11_land::{encode_terrain, terrain_digest};
use crate::checks::v7_replay::{fate_code, packed_fates};
use crate::clash_input::{
    provisional_input_digest, provisional_quiet_digest, Built, ClashBuilder, ContractBuilder,
};
use crate::world::{le, Key};

const NB: u32 = plog::NO_BELL;
const DEALT: u16 = 10_000;
const UNIT: u8 = 0; // Spearman
const MARCH_STAMINA: u16 = 74;
const KEEPER_SEED: [u8; 32] = [0xBE; 32];
const RELAY_SEED: [u8; 32] = [0xEE; 32];

/// TRANSIT_SETTLED's pairs: `(tip_to, tip, fee_to, fee, bond_to, bond,
/// reward_to, reward, pool_owed_delta)`.
type Pays = (Vec<u8>, u64, Vec<u8>, u64, Vec<u8>, u64, Vec<u8>, u64, u64);

/// Players' budget (relay-sponsored, priority 0).
const PLAYER: TxBudget = TxBudget {
    cu_limit: 60_000,
    cu_price: 0,
    loaded_limit: 1 << 20,
    heap: None,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SealKind {
    Valid,
    Garbage,
    BadPlain,
}

struct Cit {
    seed: [u8; 32],
    kp: Keypair,
    addr: Key,
    tag15: [u8; 15],
    tag8: u64,
    faction: u8,
    shard: u8,
    site: (i32, i32, u8),
    gen: u8,
    seq: u32,
}

struct March {
    host: u64,
    cit: usize,
    origin: (i32, i32),
    depart: u32,
    arrive: u32,
    dest: (i32, i32),
    tip: u64,
    plain: [u8; 37],
    salt: [u8; 32],
    ct_hash: [u8; 32],
    seal: [u8; 165],
    commit: [u8; 32],
    transit_slot: u8,
    dep_mass: u32,
    faction: u8,
    after: Option<(u32, u16)>,
    /// `(slot i, beneficiary)` once revealed.
    slot: Option<(u8, Address)>,
    settled: bool,
}

struct St {
    g: Gen,
    region: u8,
    prov: Vec<(i32, i32)>,
    ring_seeds: BTreeMap<u16, [u8; 32]>,
    cits: Vec<Cit>,
    marches: Vec<March>,
    /// THE anchors posted: bell → (A, slot, round).
    anchors: BTreeMap<u32, (i64, u64, u64)>,
    /// Seeds posted: bell → (seed, round).
    seeds: BTreeMap<u32, ([u8; 32], u64)>,
    want_seed: BTreeSet<u32>,
    next_anchor: u32,
    anchor_upto: u32,
    next: BTreeMap<(i32, i32), u32>,
    /// Revealed arrivals per (P, Q, bell).
    arrivals: BTreeMap<(i32, i32, u32), Vec<usize>>,
    /// The faction slots per (P, Q, bell, faction): march indices.
    slots: BTreeMap<(i32, i32, u32, u8), [Option<usize>; 4]>,
    keeper_budget_price: u64,
}

fn i16b(v: i32) -> [u8; 2] {
    (v as i16).to_le_bytes()
}

impl St {
    fn pk(&self, pq: (i32, i32)) -> Key {
        self.g.ctx.province(pq.0, pq.1)
    }
    fn keeper(&self) -> Address {
        self.g.keeper.pubkey()
    }

    // ------------------------------------------------------------ beacons

    /// Posts every anchor and seed due by `t`, then moves the Clock to `t`.
    fn advance_to(&mut self, t: i64) {
        loop {
            let a_due = (self.next_anchor <= self.anchor_upto)
                .then(|| self.g.bell_end(self.next_anchor) + 2);
            let s_due = self
                .want_seed
                .iter()
                .filter(|b| !self.seeds.contains_key(b))
                .filter_map(|b| {
                    self.anchors
                        .get(b)
                        .map(|a| (*b, self.g.round_time(self.g.seed_round(*b, a.0)) + 2))
                })
                .min_by_key(|x| x.1);
            let next = match (a_due, s_due) {
                (Some(a), Some((b, s))) if s < a => Some((s, Some(b))),
                (Some(a), _) => Some((a, None)),
                (None, Some((b, s))) => Some((s, Some(b))),
                (None, None) => None,
            };
            match next {
                Some((at, which)) if at <= t => {
                    self.g.at(at);
                    match which {
                        Some(b) => self.post_seed(b),
                        None => {
                            let b = self.next_anchor;
                            self.post_anchor(b);
                            self.next_anchor += 1;
                        }
                    }
                }
                _ => break,
            }
        }
        self.g.at(t);
    }

    fn post_anchor(&mut self, b: u32) {
        let r = self.region;
        let round = self.g.tlock_round(b);
        let arg = self.g.beacon(round);
        let k = self.g.ctx.bell_anchor(b, r);
        self.g.make(k, AccountKind::BellAnchor);
        let (a, slot) = (self.g.time + 1, self.g.slot + 1);
        let kp = self.keeper();
        {
            let g = &mut self.g;
            g.put(&k, BA::BELL, &b.to_le_bytes());
            g.put(&k, BA::REGION, &[r, 2]);
            g.put(&k, BA::ROUND, &round.to_le_bytes());
            g.put(&k, BA::A, &a.to_le_bytes());
            g.put(&k, BA::SLOT, &slot.to_le_bytes());
            g.put(&k, BA::SIG48, &arg.sig48);
            g.put(&k, BA::RENT_TO, &kp.to_bytes());
        }
        let mut key = b.to_le_bytes().to_vec();
        key.push(r);
        let mut pl = round.to_le_bytes().to_vec();
        pl.extend(a.to_le_bytes());
        pl.extend(slot.to_le_bytes());
        pl.extend(kp.to_bytes());
        let bell = self.g.bell();
        let body = self.g.rec(Kind::ANCHOR, bell, &key, &pl, &[]);
        let ixs = [ix::post_anchor(&self.g.a, kp, r, b, &arg, &kp)];
        let keeper = self.keeper_kp();
        self.g.send(&ixs, &[&keeper], &OP, vec![body]);
        self.anchors.insert(b, (a, slot, round));
    }

    fn post_seed(&mut self, b: u32) {
        let r = self.region;
        let (a, _, _) = self.anchors[&b];
        let round = self.g.seed_round(b, a);
        let arg = self.g.beacon(round);
        let sig96 = fclient::beacon::decompress_sig(&arg.sig48).expect("sig");
        let seed = fclient::beacon::seed_of(round, &sig96);
        let k = self.g.ctx.seed_cache(b, r, 0);
        self.g.make(k, AccountKind::SeedCache);
        let kp = self.keeper();
        let ak = self.g.ctx.bell_anchor(b, r);
        let slot = self.g.slot + 1;
        {
            let g = &mut self.g;
            g.put(&k, SC::BELL, &b.to_le_bytes());
            g.put(&k, SC::REGION, &[r, 0]);
            g.put(&k, SC::ROUND, &round.to_le_bytes());
            g.put(&k, SC::SEED, &seed);
            g.put(&k, SC::ANCHOR_KEY, &ak);
            g.put(&k, SC::A, &a.to_le_bytes());
            g.put(&k, SC::SLOT, &slot.to_le_bytes());
            g.put(&k, SC::RENT_TO, &kp.to_bytes());
        }
        let mut key = b.to_le_bytes().to_vec();
        key.extend([r, 0]);
        let mut pl = round.to_le_bytes().to_vec();
        pl.extend(seed);
        pl.extend(a.to_le_bytes());
        let bell = self.g.bell();
        let body = self.g.rec(Kind::SEED, bell, &key, &pl, &[]);
        let ixs = [ix::post_seed(&self.g.a, kp, r, b, 0, &arg, &kp)];
        let keeper = self.keeper_kp();
        self.g.send(&ixs, &[&keeper], &OP, vec![body]);
        self.seeds.insert(b, (seed, round));
    }

    fn keeper_kp(&self) -> Keypair {
        Keypair::new_from_array(KEEPER_SEED)
    }
    fn relay_kp(&self) -> Keypair {
        Keypair::new_from_array(RELAY_SEED)
    }

    /// Waits until the seed of bell `b` is posted.
    fn seed_now(&mut self, b: u32) {
        self.want_seed.insert(b);
        while !self.seeds.contains_key(&b) {
            let t = self.g.time + 30;
            self.advance_to(t);
        }
    }

    // ------------------------------------------------------------ provinces

    fn quiet_at(&self, pq: (i32, i32), b: u32) -> bool {
        let d = self.g.get(&self.pk(pq));
        ContractBuilder
            .build(d, &[], b, &[0; 32])
            .and_then(|x| x.quiet())
            .unwrap_or(false)
    }

    /// Applies what the resolve or skip of bell `b` does to the Province
    /// (the synthetic write-back, W4-D notes §3).
    fn write_back(&mut self, pq: (i32, i32), b: u32, o: Option<(&Built, &ClashOutcome)>) {
        let pk = self.pk(pq);
        let mut d = self.g.get(&pk).to_vec();
        if let Some((built, out)) = o {
            for i in 0..PV::ENTRIES_N {
                let Ok(mut e) = read_entry(&d, i) else {
                    continue;
                };
                if e.state != E::STATE_ROSTER || e.from_bell > b {
                    continue;
                }
                if let Some(fr) = out.fighter(e.id) {
                    if matches!(fr.fate, Fate::Destroyed) || fr.troops == 0 {
                        write_entry(&mut d, i, &Entry::FREE).expect("entry");
                        continue;
                    }
                    let mut h = e.to_host().expect("host");
                    h.apply_clash(b, fr.troops, fr.stamina, fr.engaged)
                        .expect("apply");
                    e.set_host(&h);
                    if let Fate::Stays { tile } | Fate::Withdrew { tile } = fr.fate {
                        e.tile = tile;
                    }
                    write_entry(&mut d, i, &e).expect("entry");
                }
            }
            for a in &built.arrivals {
                let Some(fr) = out.fighter(a.id) else {
                    continue;
                };
                if let Fate::Stays { tile } | Fate::Withdrew { tile } = fr.fate {
                    let h = Host {
                        id: a.id,
                        owner: holding_key_of_host(a.id),
                        faction: a.faction,
                        unit: a.unit,
                        troops: fr.troops,
                        stamina: Stamina {
                            value: fr.stamina,
                            bell: b,
                        },
                        ready_bell: if fr.engaged {
                            ready_bell_after(b)
                        } else {
                            b + 1
                        },
                        pending: None,
                    };
                    let e = Entry::from_host(&h, tile, E::STATE_ROSTER, a.dealt_bps as u16, b + 1);
                    let i = (0..PV::ENTRIES_N)
                        .find(|&i| read_entry(&d, i).is_ok_and(|x| x.state == E::STATE_FREE))
                        .expect("room");
                    write_entry(&mut d, i, &e).expect("entry");
                }
            }
            for gr in &out.garrisons {
                let cg = u32::from_le_bytes(
                    d[PV::CAMP + CP::GEN..PV::CAMP + CP::GEN + 4]
                        .try_into()
                        .expect("4"),
                );
                if gr.id == u64::MAX - cg as u64 {
                    d[PV::CAMP + CP::TROOPS..PV::CAMP + CP::TROOPS + 4]
                        .copy_from_slice(&gr.troops.to_le_bytes());
                    if gr.troops == 0 {
                        d[PV::CAMP + CP::STATE] = CP::STATE_NONE;
                    }
                }
            }
            d[PV::LAST_DIGEST..PV::LAST_DIGEST + 32].copy_from_slice(&out.digest());
        }
        // Pending changes of bell b settle.
        for i in 0..PV::ENTRIES_N {
            let Ok(mut e) = read_entry(&d, i) else {
                continue;
            };
            match (e.state, e.op) {
                (E::STATE_ROSTER, EntryOp::Spend { .. }) if e.pend_bell == b => {
                    let mut h = e.to_host().expect("host");
                    h.settle(b + 1).expect("settle");
                    e.set_host(&h);
                    e.state = E::STATE_DEPARTED;
                    write_entry(&mut d, i, &e).expect("entry");
                }
                (_, EntryOp::Forfeit) if e.pend_bell == b => {
                    write_entry(&mut d, i, &Entry::FREE).expect("entry")
                }
                (E::STATE_MUSTER_PENDING, _) if e.from_bell == b + 1 => {
                    e.state = E::STATE_ROSTER;
                    write_entry(&mut d, i, &e).expect("entry");
                }
                _ => {}
            }
        }
        d[PV::RESOLVED_NEXT..PV::RESOLVED_NEXT + 4].copy_from_slice(&(b + 1).to_le_bytes());
        let ep = PV::ROSTER_EPOCH;
        let n = le(&d[ep..ep + 4]) as u32 + 1;
        d[ep..ep + 4].copy_from_slice(&n.to_le_bytes());
        *self.g.d(&pk) = d;
        self.next.insert(pq, b + 1);
    }

    /// Emits the SkipQuiet of the bells in `run` (their write-back is
    /// already applied, bell by bell, as SkipQuiet does).
    fn flush_skip(&mut self, pq: (i32, i32), run: &mut Vec<u32>) {
        if run.is_empty() {
            return;
        }
        let (b0, n) = (run[0], run.len() as u8);
        let pk = self.pk(pq);
        let mut pl = b0.to_le_bytes().to_vec();
        pl.push(n);
        pl.extend(provisional_quiet_digest(pq.0, pq.1, b0, n));
        let key = [pq.0.to_le_bytes(), pq.1.to_le_bytes()].concat();
        let bell = self.g.bell();
        let body = self
            .g
            .rec(Kind::SKIP, bell, &key, &pl, &[(En::Province, pk)]);
        let src = vec![AnchorSource::Anchor; n as usize];
        let ixs = [ix::skip_quiet(&self.g.a, self.keeper(), pq, b0, n, &src)];
        let k = self.keeper_kp();
        self.g.send(&ixs, &[&k], &OP, vec![body]);
        run.clear();
    }

    /// Resolves or skips `pq` through bell `upto` (inclusive).
    fn resolve_through(&mut self, pq: (i32, i32), upto: u32) {
        let mut run: Vec<u32> = vec![];
        let start = self.next[&pq];
        for b in start..=upto {
            assert!(
                self.anchors.contains_key(&b),
                "anchor {b} before resolving it"
            );
            let close = self.anchors[&b].0 + self.g.p.reveal_window as i64;
            if self.g.time < close {
                let t = close + 1;
                self.advance_to(t);
            }
            let has = self
                .arrivals
                .get(&(pq.0, pq.1, b))
                .is_some_and(|v| !v.is_empty());
            if !has && self.quiet_at(pq, b) {
                self.write_back(pq, b, None);
                run.push(b);
                if run.len() == 24 {
                    self.flush_skip(pq, &mut run);
                }
                continue;
            }
            self.flush_skip(pq, &mut run);
            self.clash(pq, b);
        }
        self.flush_skip(pq, &mut run);
    }

    /// GatherClash and ResolveFromInputs of `(pq, b)`.
    fn clash(&mut self, pq: (i32, i32), b: u32) {
        self.seed_now(b);
        let (p, q) = pq;
        let pk = self.pk(pq);
        let ck = self.g.ctx.clash_inputs(p, q, b);
        self.g.make(ck, AccountKind::ClashInputs);
        let kp = self.keeper();
        let present: Vec<(usize, usize)> = (0..6u8)
            .flat_map(|f| {
                let s = self.slots.get(&(p, q, b, f)).copied().unwrap_or([None; 4]);
                (0..4).filter_map(move |i| s[i].map(|m| (f as usize * 4 + i, m)))
            })
            .collect();
        {
            let g = &mut self.g;
            g.put(&ck, CI::P, &i16b(p));
            g.put(&ck, CI::Q, &i16b(q));
            g.put(&ck, CI::BELL, &b.to_le_bytes());
            g.put(&ck, CI::ARRIVALS_MASK, &CI::ALL_GATHERED.to_le_bytes());
            g.put(&ck, CI::RENT_TO, &kp.to_bytes());
        }
        let no_arrivals = present.is_empty();
        if no_arrivals {
            self.g.d(&ck)[CI::FLAGS] = CI::FLAG_NO_ARRIVALS;
        }
        let mut holdings = vec![];
        for &(pos, mi) in &present {
            let m = &self.marches[mi];
            let (troops, stamina) = m.after.expect("departure settled before the gather");
            let sk = self.g.ctx.arrival_slot(p, q, b, m.faction, (pos % 4) as u8);
            let sd = self.g.get(&sk).to_vec();
            let o = CI::arrival(pos);
            let g = &mut self.g;
            g.put(&ck, o + AR::HOST_ID, &m.host.to_le_bytes());
            g.put(
                &ck,
                o + AR::CITIZEN_TAG,
                &sd[AS::CITIZEN_TAG..AS::CITIZEN_TAG + 8],
            );
            g.put(&ck, o + AR::DEP_MASS, &m.dep_mass.to_le_bytes());
            g.put(&ck, o + AR::TROOPS, &troops.to_le_bytes());
            g.put(&ck, o + AR::STAMINA, &stamina.to_le_bytes());
            g.put(
                &ck,
                o + AR::RETREAT,
                &sd[AS::RETREAT_BPS..AS::RETREAT_BPS + 2],
            );
            g.put(&ck, o + AR::DEALT, &sd[AS::DEALT_BPS..AS::DEALT_BPS + 2]);
            g.put(
                &ck,
                o + AR::FACTION,
                &[m.faction, sd[AS::UNIT], sd[AS::TILE], sd[AS::STANCE], 1],
            );
            holdings.push(Address::new_from_array(
                g.ctx.holding_of_host(m.host).expect("holding"),
            ));
        }
        self.g.d(&ck)[CI::N_PRESENT] = present.len() as u8;
        let key = [p.to_le_bytes(), q.to_le_bytes(), b.to_le_bytes()].concat();
        let mask: u32 = present.iter().fold(0, |m, (pos, _)| m | 1 << pos);
        let mut pl = vec![0u8, 24];
        pl.extend(mask.to_le_bytes());
        pl.push(no_arrivals as u8);
        let bell = self.g.bell();
        let body = self
            .g
            .rec(Kind::GATHER, bell, &key, &pl, &[(En::ClashInputs, ck)]);
        let positions: Vec<(u8, u8)> = if no_arrivals {
            vec![]
        } else {
            (0..24).map(|k| ((k / 4) as u8, (k % 4) as u8)).collect()
        };
        let bitmap = if no_arrivals { 0 } else { mask };
        let ixs = [ix::gather_clash(
            &self.g.a,
            kp,
            pq,
            b,
            AnchorSource::Anchor,
            0,
            &positions,
            &holdings,
            bitmap,
            &kp,
        )];
        let k = self.keeper_kp();
        self.g.send(&ixs, &[&k], &OP, vec![body]);
        // Resolve.
        let t = self.g.time + 3;
        self.advance_to(t);
        let (seed, _) = self.seeds[&b];
        let built = ContractBuilder
            .build(self.g.get(&pk), self.g.get(&ck), b, &seed)
            .expect("the synthetic inputs build");
        let out = built.resolve().expect("resolves");
        let fates = packed_fates(&built, &out);
        for (a, &pos) in built.arrivals.iter().zip(&built.arrival_pos) {
            if let Some(fr) = out.fighter(a.id) {
                let o = CI::arrival(pos);
                self.g.d(&ck)[o + AR::FATE] = fate_code(&fr.fate);
                self.g
                    .put(&ck, o + AR::TROOPS_AFTER, &fr.troops.to_le_bytes());
            }
        }
        let rts = (self.g.time + 1 - self.g.genesis_ts) as u32;
        {
            let g = &mut self.g;
            g.d(&ck)[CI::FLAGS] |= CI::FLAG_RESOLVED;
            g.put(&ck, CI::RESOLVER, &kp.to_bytes());
            let s = g.slot + 1;
            g.put(&ck, CI::EV_SLOT, &s.to_le_bytes());
            g.put(&ck, CI::EV_LIMIT, &OP.cu_limit.to_le_bytes());
            g.put(&ck, CI::RESOLVED_TS, &rts.to_le_bytes());
        }
        self.write_back(pq, b, Some((&built, &out)));
        let mut pl = out.digest().to_vec();
        pl.extend(provisional_input_digest(self.g.get(&ck), b));
        pl.extend(out.engagements.to_le_bytes());
        pl.extend(fates);
        let bell = self.g.bell();
        let body = self.g.rec(
            Kind::CLASH,
            bell,
            &key,
            &pl,
            &[(En::Province, pk), (En::ClashInputs, ck)],
        );
        let ixs = [ix::resolve_from_inputs(
            &self.g.a,
            kp,
            pq,
            b,
            SeedSource::Cache { nonce: 0 },
            &kp,
        )];
        self.g.send(&ixs, &[&k], &OP, vec![body]);
    }

    // ------------------------------------------------------------ players

    fn player(&self, c: usize) -> Player {
        Player {
            actor: self.cits[c].kp.pubkey(),
            payer: self.g.relay.pubkey(),
            wallet: self.cits[c].kp.pubkey(),
        }
    }

    fn href(&self, c: usize) -> HoldingRef {
        let (p, q, s) = self.cits[c].site;
        HoldingRef {
            p: p as i16,
            q: q as i16,
            site: s,
        }
    }

    fn signers(&self, c: usize) -> (Keypair, Keypair) {
        (self.relay_kp(), Keypair::new_from_array(self.cits[c].seed))
    }

    /// Free passable tiles of a province (no site, no camp).
    fn tiles(&self, pq: (i32, i32)) -> Vec<u8> {
        let d = self.g.get(&self.pk(pq));
        let pv = fclient::decode::Province::decode(d).expect("province");
        (0..61u8)
            .filter(|t| pv.passable_mask & (1 << t) != 0)
            .filter(|t| !pv.sites[..pv.site_count as usize].contains(t))
            .filter(|t| !(pv.camp.state == 1 && pv.camp.tile == *t))
            .collect()
    }

    fn muster(&mut self, c: usize, troops: u32, tile: u8) -> u64 {
        let b = self.g.bell();
        let (p, q, s) = self.cits[c].site;
        self.cits[c].seq += 1;
        let id = host_id(p, q, s, self.cits[c].gen, self.cits[c].seq).expect("host id");
        let pk = self.pk((p, q));
        let hk = self.g.ctx.holding(p, q, s);
        let h = Host::muster(
            id,
            holding_key_of_host(id),
            self.cits[c].faction,
            frontier_abi::entry::UNITS[UNIT as usize],
            troops * 1_000,
            b,
        )
        .expect("muster");
        let e = Entry::from_host(&h, tile, E::STATE_MUSTER_PENDING, DEALT, b + 1);
        let i = (0..PV::ENTRIES_N)
            .find(|&i| read_entry(self.g.get(&pk), i).is_ok_and(|x| x.state == E::STATE_FREE))
            .expect("room");
        write_entry(self.g.d(&pk), i, &e).expect("entry");
        let seq = self.cits[c].seq;
        self.g.put(&hk, H::HOST_SEQ, &seq.to_le_bytes());
        let mut pl = vec![UNIT];
        pl.extend(troops.to_le_bytes());
        pl.extend([tile, i as u8]);
        let cit = self.cits[c].addr;
        let body = self.g.rec(
            Kind::MUSTER,
            b,
            &id.to_le_bytes(),
            &pl,
            &[(En::Citizen, cit), (En::Holding, hk), (En::Province, pk)],
        );
        let ixs = [ix::muster(
            &self.g.a,
            &self.player(c),
            self.href(c),
            UNIT,
            troops,
            tile,
        )];
        let (r, w) = self.signers(c);
        self.g.send(&ixs, &[&r, &w], &PLAYER, vec![body]);
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn depart(
        &mut self,
        c: usize,
        host: u64,
        arrive: u32,
        dest: (i32, i32),
        tile: u8,
        stance: u8,
        tip: u64,
        kind: SealKind,
    ) -> usize {
        let b = self.g.bell();
        let (p, q, s) = self.cits[c].site;
        let pk = self.pk((p, q));
        let hk = self.g.ctx.holding(p, q, s);
        let i = frontier_abi::entry::find_entry(self.g.get(&pk), host).expect("host entry");
        let mut e = read_entry(self.g.get(&pk), i).expect("entry");
        let mut h = e.to_host().expect("host");
        let rn = self.next[&(p, q)];
        h.depart(b, MARCH_STAMINA, rn).expect("depart");
        e.set_host(&h);
        write_entry(self.g.d(&pk), i, &e).expect("entry");
        let mut pt = fseal::unpack(&[0u8; 37]);
        pt.version = 1;
        pt.host_id = host;
        pt.arrive_bell = arrive;
        pt.dest_p = dest.0 as i16;
        pt.dest_q = dest.1 as i16;
        pt.dest_tile = if kind == SealKind::BadPlain { 70 } else { tile };
        pt.stance = stance;
        pt.path_len = 1;
        pt.path = fseal::path_of(&[0]);
        let plain = fseal::pack(&pt);
        let round = self.g.tlock_round(arrive);
        let k16: [u8; 16] = sha256(&[b"synthetic-seal-key", &host.to_le_bytes()])[..16]
            .try_into()
            .expect("16");
        let sealed = fseal::seal_with_key(&plain, &self.g.key.pk96, round, &k16).expect("seal");
        let (seal, commit, salt, ct_hash) = match kind {
            SealKind::Garbage => {
                // garbage_seal: random 165 B with a valid commitment to a
                // valid plaintext.
                let mut g = [0u8; 165];
                for (j, ch) in g.chunks_mut(32).enumerate() {
                    let x = sha256(&[b"garbage", &host.to_le_bytes(), &[j as u8]]);
                    ch.copy_from_slice(&x[..ch.len()]);
                }
                g[0] = 0xA0 | (g[0] & 0x1F); // a compressed-G2 flag byte (syntax only)
                let salt = sha256(&[b"garbage-salt", &host.to_le_bytes()]);
                (g, fseal::commit(&plain, &salt), salt, fseal::ct_hash(&g))
            }
            _ => (sealed.seal, sealed.commit, sealed.salt, sealed.ct_hash),
        };
        let root = fseal::seal_root(&commit, &ct_hash);
        let ts = (0..4u8)
            .find(|t| {
                self.marches
                    .iter()
                    .all(|m| m.settled || m.cit != c || m.transit_slot != *t)
            })
            .expect("transit slot");
        let off = H::TRANSIT + ts as usize * T::SIZE;
        {
            let g = &mut self.g;
            g.d(&hk)[off + T::STATE] = T::STATE_DEPARTED;
            g.put(&hk, off + T::HOST_ID, &host.to_le_bytes());
            g.put(&hk, off + T::DEPART_BELL, &b.to_le_bytes());
            g.put(&hk, off + T::ARRIVE_BELL, &arrive.to_le_bytes());
            g.put(&hk, off + T::DEP_MASS, &e.troops.to_le_bytes());
            g.put(&hk, off + T::SEAL_ROOT, &root);
            g.put(&hk, off + T::TIP, &tip.to_le_bytes());
            if let Some(a) = g.acc.get_mut(&hk) {
                a.lamports += tip + g.p.march_fee + g.p.seal_bond;
            }
        }
        let mut pl = p.to_le_bytes().to_vec();
        pl.extend(q.to_le_bytes());
        pl.push(e.tile);
        pl.extend(b.to_le_bytes());
        pl.extend(arrive.to_le_bytes());
        pl.extend(e.troops.to_le_bytes());
        pl.extend(MARCH_STAMINA.to_le_bytes());
        pl.extend(tip.to_le_bytes());
        pl.extend(root);
        pl.extend(commit);
        pl.extend(seal);
        let cit = self.cits[c].addr;
        let body = self.g.rec(
            Kind::DEPART,
            b,
            &host.to_le_bytes(),
            &pl,
            &[(En::Citizen, cit), (En::Holding, hk), (En::Province, pk)],
        );
        let args = DepartArgs {
            host_id: host,
            commit,
            seal,
            arrive_bell: arrive,
            tip,
            transit_slot: ts,
        };
        let ixs = [ix::depart(
            &self.g.a,
            &self.player(c),
            self.href(c),
            (p as i16, q as i16),
            &args,
        )];
        let (r, w) = self.signers(c);
        self.g.send(&ixs, &[&r, &w], &PLAYER, vec![body]);
        self.marches.push(March {
            host,
            cit: c,
            origin: (p, q),
            depart: b,
            arrive,
            dest,
            tip,
            plain,
            salt,
            ct_hash,
            seal,
            commit,
            transit_slot: ts,
            dep_mass: e.troops,
            faction: self.cits[c].faction,
            after: None,
            slot: None,
            settled: false,
        });
        self.marches.len() - 1
    }

    fn settle_departure(&mut self, mi: usize) {
        let (host, (p, q), c, depart, ts) = {
            let m = &self.marches[mi];
            (m.host, m.origin, m.cit, m.depart, m.transit_slot)
        };
        let pk = self.pk((p, q));
        let hk = self.g.ctx.holding_of_host(host).expect("holding");
        let i = frontier_abi::entry::find_entry(self.g.get(&pk), host).expect("departed entry");
        let e = read_entry(self.g.get(&pk), i).expect("entry");
        assert_eq!(e.state, E::STATE_DEPARTED);
        let h = e.to_host().expect("host");
        let (troops, stamina) = h
            .march_values(depart, self.next[&(p, q)])
            .expect("march values");
        write_entry(self.g.d(&pk), i, &Entry::FREE).expect("entry");
        let off = H::TRANSIT + ts as usize * T::SIZE;
        self.g.d(&hk)[off + T::STATE] = T::STATE_SETTLED;
        self.g
            .put(&hk, off + T::TROOPS_AFTER, &troops.to_le_bytes());
        self.g
            .put(&hk, off + T::STAMINA_AFTER, &stamina.to_le_bytes());
        let mut pl = troops.to_le_bytes().to_vec();
        pl.extend(stamina.to_le_bytes());
        pl.push(0);
        let bell = self.g.bell();
        let body = self.g.rec(
            Kind::DEPARTURE_SETTLED,
            bell,
            &host.to_le_bytes(),
            &pl,
            &[(En::Holding, hk), (En::Province, pk)],
        );
        let ixs = [ix::settle_departure(
            &self.g.a,
            self.keeper(),
            (p as i16, q as i16),
            self.href(c),
            ts,
        )];
        let k = self.keeper_kp();
        self.g.send(&ixs, &[&k], &OP, vec![body]);
        self.marches[mi].after = Some((troops, stamina));
    }

    /// Reveal of march `mi` by `by` (the keeper, or the owner in-bell) with
    /// a compute-unit price `price`. `false`: refused (`QuotaRefused`).
    fn reveal(&mut self, mi: usize, owner: bool, price: u64) -> bool {
        let (host, (p, q), arrive, fct, c) = {
            let m = &self.marches[mi];
            (m.host, m.dest, m.arrive, m.faction, m.cit)
        };
        let tag8 = self.cits[c].tag8;
        let mass = self.marches[mi].dep_mass;
        let cur = self
            .slots
            .get(&(p, q, arrive, fct))
            .copied()
            .unwrap_or([None; 4]);
        let mut fs: FactionSlots = [None; 4];
        for i in 0..4 {
            fs[i] = cur[i].map(|m| SlotEntry {
                host_id: self.marches[m].host,
                citizen: self.cits[self.marches[m].cit].tag8,
                troops: self.marches[m].dep_mass,
            });
        }
        let me = SlotEntry {
            host_id: host,
            citizen: tag8,
            troops: mass,
        };
        let dec = admit_arrival(ProvinceCoord::new(p, q), arrive, &fs, me);
        let payer = if owner {
            Keypair::new_from_array(self.cits[c].seed)
        } else {
            self.keeper_kp()
        };
        let ben = payer.pubkey();
        let day = day_of(arrive);
        let dk = self.g.ctx.arrival_day(p, q, day);
        let (bit_o, bit_m) = AD::bit(arrive);
        let bit_set = self.g.has(&dk) && self.g.get(&dk)[bit_o] & bit_m != 0;
        let target = match dec {
            SlotDecision::Fill { slot } | SlotDecision::Displace { slot, .. } => slot,
            SlotDecision::Refuse(_) => 0,
        };
        let m = &self.marches[mi];
        let args = RevealArgs {
            holding: self.href(c),
            transit_slot: m.transit_slot,
            target_i: target,
            plain: m.plain,
            salt: m.salt,
            ct_hash: m.ct_hash,
            beneficiary: ben,
            dest: (p, q),
            arrive,
            faction: fct,
            day_writable: !bit_set,
            path_provinces: vec![],
        };
        let budget = TxBudget {
            cu_limit: self.g.p.reveal_cu_limit,
            cu_price: price,
            loaded_limit: self.g.p.reveal_loaded_limit,
            heap: None,
        };
        let ixs = [ix::reveal(&self.g.a, ben, &args)];
        let (slot_i, displaced) = match dec {
            SlotDecision::Refuse(_) => {
                self.g.fail(
                    &ixs,
                    &[&payer],
                    &budget,
                    frontier_abi::error::FrontierError::QuotaRefused as u32,
                );
                return false;
            }
            SlotDecision::Fill { slot } => (slot, None),
            SlotDecision::Displace { slot, displaced } => (slot, Some(displaced.host_id)),
        };
        let sk = self.g.ctx.arrival_slot(p, q, arrive, fct, slot_i);
        let created_day = !bit_set;
        if !self.g.has(&dk) {
            self.g.make(dk, AccountKind::ArrivalDay);
            self.g.put(&dk, AD::P, &i16b(p));
            self.g.put(&dk, AD::Q, &i16b(q));
            self.g.put(&dk, AD::DAY, &day.to_le_bytes());
            self.g.put(&dk, AD::RENT_TO, &ben.to_bytes());
        }
        self.g.d(&dk)[bit_o] |= bit_m;
        let new_slot = displaced.is_none();
        if new_slot {
            self.g.make(sk, AccountKind::ArrivalSlot);
            self.g.put(&sk, AS::P, &i16b(p));
            self.g.put(&sk, AS::Q, &i16b(q));
            self.g.put(&sk, AS::BELL, &arrive.to_le_bytes());
            self.g.d(&sk)[AS::FACTION] = fct;
            self.g.d(&sk)[AS::I] = slot_i;
            self.g.put(&sk, AS::RENT_TO, &ben.to_bytes());
        }
        let pt = fseal::unpack(&self.marches[mi].plain);
        let ev_slot = self.g.slot + 1;
        {
            let g = &mut self.g;
            g.d(&sk)[AS::UNIT] = UNIT;
            g.d(&sk)[AS::STANCE] = pt.stance;
            g.d(&sk)[AS::TILE] = pt.dest_tile;
            g.d(&sk)[AS::FLAGS] = if created_day && new_slot {
                AS::FLAG_CREATED_DAY
            } else {
                0
            };
            g.put(&sk, AS::RETREAT_BPS, &pt.retreat_bps.to_le_bytes());
            g.put(&sk, AS::HOST_ID, &host.to_le_bytes());
            g.put(&sk, AS::CITIZEN_TAG, &tag8.to_le_bytes());
            g.put(&sk, AS::DEP_MASS, &mass.to_le_bytes());
            g.put(&sk, AS::DEALT_BPS, &DEALT.to_le_bytes());
            g.put(&sk, AS::BENEFICIARY, &ben.to_bytes());
            g.put(&sk, AS::EV_SLOT, &ev_slot.to_le_bytes());
            g.put(&sk, AS::EV_PRICE, &price.to_le_bytes());
            g.put(&sk, AS::EV_LIMIT, &budget.cu_limit.to_le_bytes());
            g.put(&sk, AS::EV_LOADED, &budget.loaded_limit.to_le_bytes());
            g.d(&sk)[AS::CLAIMED] = 0;
        }
        let key = [
            p.to_le_bytes().as_slice(),
            &q.to_le_bytes(),
            &arrive.to_le_bytes(),
            &[fct, slot_i],
        ]
        .concat();
        let mut pl = host.to_le_bytes().to_vec();
        pl.extend([pt.dest_tile, pt.stance]);
        pl.extend(pt.retreat_bps.to_le_bytes());
        pl.push(displaced.is_some() as u8);
        pl.extend(displaced.unwrap_or(0).to_le_bytes());
        pl.extend(ben.to_bytes());
        pl.extend(ev_slot.to_le_bytes());
        pl.extend(price.to_le_bytes());
        pl.extend(budget.cu_limit.to_le_bytes());
        pl.push(created_day as u8);
        let bell = self.g.bell();
        let body = self.g.rec(Kind::REVEAL, bell, &key, &pl, &[]);
        self.g.send(&ixs, &[&payer], &budget, vec![body]);
        // The quota's own bookkeeping.
        let mut fs2 = fs;
        apply_slot(&mut fs2, me, dec);
        let e = self.slots.entry((p, q, arrive, fct)).or_insert([None; 4]);
        if let Some(d) = displaced {
            let di = self
                .marches
                .iter()
                .position(|x| x.host == d && x.arrive == arrive)
                .expect("displaced march");
            self.marches[di].slot = None;
            self.arrivals
                .entry((p, q, arrive))
                .or_default()
                .retain(|x| *x != di);
        }
        e[slot_i as usize] = Some(mi);
        self.marches[mi].slot = Some((slot_i, ben));
        self.arrivals.entry((p, q, arrive)).or_default().push(mi);
        true
    }

    /// SettleTransit of march `mi` (§5.11 D5, the synthetic encoding).
    fn settle_transit(&mut self, mi: usize, archived: bool) {
        let (host, (p, q), arrive, c, fct) = {
            let m = &self.marches[mi];
            (m.host, m.dest, m.arrive, m.cit, m.faction)
        };
        let kp = self.keeper();
        let sig = self.g.key.sign(self.g.tlock_round(arrive));
        let m = &self.marches[mi];
        let (code, _) = fseal::judge(&m.seal, &m.commit, &sig, host, arrive);
        let ck = self.g.ctx.clash_inputs(p, q, arrive);
        let ci = self
            .g
            .acc
            .get(&ck)
            .map(|a| a.data.clone())
            .filter(|d| d[CI::FLAGS] & CI::FLAG_RESOLVED != 0);
        let rent = self.g.relay.pubkey().to_bytes();
        let (fee, bond, tip) = (self.g.p.march_fee, self.g.p.seal_bond, m.tip);
        let resolver: [u8; 32] = ci
            .as_ref()
            .map(|d| d[CI::RESOLVER..CI::RESOLVER + 32].try_into().expect("32"))
            .unwrap_or(kp.to_bytes());
        let slot_ben = m.slot.map(|s| s.1.to_bytes()).unwrap_or(kp.to_bytes());
        let slot_i = m.slot.map(|s| s.0).unwrap_or(0);
        let in_rec = ci.as_ref().and_then(|d| {
            (0..24).find(|&k| {
                let o = CI::arrival(k);
                d[o + AR::PRESENT] == 1 && le(&d[o + AR::HOST_ID..o + AR::HOST_ID + 8]) == host
            })
        });
        let pre8 = |k: &[u8; 32]| k[..8].to_vec();
        let (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool): Pays;
        let z = vec![0u8; 8];
        let (outcome, troops, mut chained): (u8, u32, Vec<(En, Key)>);
        let hk = self.g.ctx.holding_of_host(host).expect("holding");
        chained = vec![(En::Holding, hk)];
        if code > 0 {
            outcome = transit_outcome::BAD_SEAL;
            troops = 0;
            (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool) = (
                z.clone(),
                0,
                z.clone(),
                0,
                z.clone(),
                0,
                pre8(&kp.to_bytes()),
                tip + fee + bond,
                0,
            );
            // A bad seal still standing at the destination: Forfeit.
            let pk = self.pk((p, q));
            if let Some(i) = frontier_abi::entry::find_entry(self.g.get(&pk), host) {
                let mut e = read_entry(self.g.get(&pk), i).expect("entry");
                e.op = EntryOp::Forfeit;
                e.pend_bell = self.g.bell();
                write_entry(self.g.d(&pk), i, &e).expect("entry");
                chained.push((En::Province, pk));
            }
        } else if let (Some(d), Some(k)) = (&ci, in_rec) {
            let o = CI::arrival(k);
            outcome = d[o + AR::FATE];
            troops = match outcome {
                transit_outcome::STAYS | transit_outcome::WITHDREW => {
                    le(&d[o + AR::TROOPS_AFTER..o + AR::TROOPS_AFTER + 4]) as u32
                }
                transit_outcome::DESTROYED => 0,
                _ => le(&d[o + AR::TROOPS..o + AR::TROOPS + 4]) as u32,
            };
            (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool) = (
                pre8(&slot_ben),
                tip,
                pre8(&resolver),
                fee,
                pre8(&rent),
                bond,
                z.clone(),
                0,
                0,
            );
            self.g.d(&ck)[CI::SETTLED_MASK + k / 8] |= 1 << (k % 8);
            chained.push((En::ClashInputs, ck));
        } else if let Some(d) = &ci {
            let mut fs: FactionSlots = [None; 4];
            for (i, s) in fs.iter_mut().enumerate() {
                let o = CI::arrival(fct as usize * 4 + i);
                if d[o + AR::PRESENT] == 1 {
                    *s = Some(SlotEntry {
                        host_id: le(&d[o + AR::HOST_ID..o + AR::HOST_ID + 8]),
                        citizen: le(&d[o + AR::CITIZEN_TAG..o + AR::CITIZEN_TAG + 8]),
                        troops: le(&d[o + AR::DEP_MASS..o + AR::DEP_MASS + 4]) as u32,
                    });
                }
            }
            let me = SlotEntry {
                host_id: host,
                citizen: self.cits[c].tag8,
                troops: m.dep_mass,
            };
            let after = m.after.map(|a| a.0).unwrap_or(m.dep_mass);
            if let SlotDecision::Refuse(_) =
                admit_arrival(ProvinceCoord::new(p, q), arrive, &fs, me)
            {
                outcome = transit_outcome::BOUNCED_UNRANKED;
                troops = after;
                (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool) = (
                    pre8(&rent),
                    tip,
                    pre8(&resolver),
                    fee,
                    pre8(&rent),
                    bond,
                    z.clone(),
                    0,
                    0,
                );
            } else {
                outcome = transit_outcome::ROUTED;
                troops = rout_survivors(after);
                (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool) = (
                    z.clone(),
                    0,
                    pre8(&resolver),
                    fee,
                    pre8(&rent),
                    bond,
                    z.clone(),
                    0,
                    tip,
                );
            }
        } else {
            outcome = transit_outcome::ROUTED;
            troops = rout_survivors(m.after.map(|a| a.0).unwrap_or(m.dep_mass));
            (tx8, tipv, fx8, feev, bx8, bondv, rx8, rewv, pool) = (
                z.clone(),
                0,
                z.clone(),
                0,
                pre8(&rent),
                bond,
                z.clone(),
                0,
                tip + fee,
            );
        }
        let off = H::TRANSIT + m.transit_slot as usize * T::SIZE;
        let ts = m.transit_slot;
        let (commit, seal) = (m.commit, m.seal);
        self.g.put(&hk, off, &[0u8; T::SIZE]);
        if pool > 0 {
            let po = le(&self.g.get(&hk)[H::POOL_OWED..H::POOL_OWED + 8]) + pool;
            self.g.put(&hk, H::POOL_OWED, &po.to_le_bytes());
        }
        let mut pl = vec![outcome, code];
        pl.extend(troops.to_le_bytes());
        pl.extend(&tx8);
        pl.extend(tipv.to_le_bytes());
        pl.extend(&fx8);
        pl.extend(feev.to_le_bytes());
        pl.extend(&bx8);
        pl.extend(bondv.to_le_bytes());
        pl.extend(&rx8);
        pl.extend(rewv.to_le_bytes());
        pl.extend(pool.to_le_bytes());
        pl.push(0);
        let bell = self.g.bell();
        let body = self.g.rec(
            Kind::TRANSIT_SETTLED,
            bell,
            &host.to_le_bytes(),
            &pl,
            &chained,
        );
        let args = SettleTransitArgs {
            holding: self.href(c),
            transit_slot: ts,
            commit,
            seal,
            beneficiary: kp,
            dest: (p, q),
            arrive,
            faction: fct,
            slot_i,
            home: self.marches[mi].origin,
            anchor_present: !archived,
            slot_beneficiary: Address::new_from_array(slot_ben),
            resolver: Address::new_from_array(resolver),
            holding_rent_payer: Address::new_from_array(rent),
        };
        let ixs = [ix::settle_transit(&self.g.a, kp, &args)];
        let k = self.keeper_kp();
        self.g.send(&ixs, &[&k], &OP, vec![body]);
        self.marches[mi].settled = true;
    }
}

/// Builds the synthetic march season.
pub fn march() -> crate::Input {
    let program = Address::new_from_array([0x5F; 32]);
    let mut g = Gen::new(program, 8, 1_785_542_400);
    // ---- lifecycle: announce, create, genesis seed
    let authority = Keypair::new_from_array([0xA1; 32]);
    let auth = authority.pubkey();
    let season = g.ctx.season;
    let sp = g.p.to_bytes();
    let ph = frontier_abi::presets::params_hash(&sp, &g.payout);
    let t_create_min = g.time + 86_460;
    let bond = 1_000_000_000u64;
    g.make(season, AccountKind::Season);
    g.d(&season)[S::STATUS] = 1;
    g.put(&season, S::PARAMS_HASH, &ph);
    g.put(&season, S::T_CREATE_MIN, &t_create_min.to_le_bytes());
    let mut pl = ph.to_vec();
    pl.extend(t_create_min.to_le_bytes());
    pl.extend(bond.to_le_bytes());
    let body = g.rec(
        Kind::ANNOUNCE,
        NB,
        &8u64.to_le_bytes(),
        &pl,
        &[(En::Season, season)],
    );
    let ixs = [ix::announce_season(&g.a, auth, ph, t_create_min, bond)];
    g.send(&ixs, &[&authority], &OP, vec![body]);
    g.at(t_create_min + 5);
    g.genesis_round = beacon::genesis_seed_round(&g.clock, t_create_min, g.p.seed_margin);
    g.genesis_ts = beacon::genesis_ts(&g.clock, g.genesis_round);
    let (gr, gts) = (g.genesis_round, g.genesis_ts);
    {
        let p = g.p;
        g.d(&season)[S::STATUS] = 2;
        g.put(
            &season,
            S::RULESET_HASH,
            &frontier_abi::presets::RULESET_HASH,
        );
        g.put(&season, S::GENESIS_TS, &gts.to_le_bytes());
        g.put(&season, S::GENESIS_ROUND, &gr.to_le_bytes());
        g.put(&season, S::QUICKNET_PK_HASH, &p.quicknet_pk_hash);
        g.put(&season, S::REVEAL_WINDOW, &p.reveal_window.to_le_bytes());
        g.put(&season, S::SEED_MARGIN, &p.seed_margin.to_le_bytes());
        g.put(&season, S::MARCH_FEE, &p.march_fee.to_le_bytes());
        g.put(&season, S::SEAL_BOND, &p.seal_bond.to_le_bytes());
        g.put(&season, S::END_BELL, &p.end_bell.to_le_bytes());
        g.put(&season, S::DRAND_GENESIS, &p.drand_genesis.to_le_bytes());
        g.put(&season, S::DRAND_PERIOD, &p.drand_period.to_le_bytes());
    }
    let fk0 = g.ctx.frontier();
    g.make(fk0, AccountKind::Frontier);
    for w in 0..6u8 {
        let k = g.ctx.province_fund(w);
        g.make(k, AccountKind::ProvinceFund);
        g.d(&k)[PF::WEDGE] = w;
    }
    let dp = g.ctx.defence_pool();
    g.make(dp, AccountKind::DefencePool);
    if let Some(a) = g.acc.get_mut(&dp) {
        a.lamports += g.p.dpool_initial;
    }
    let mut pl = ph.to_vec();
    pl.extend(gr.to_le_bytes());
    pl.extend(gts.to_le_bytes());
    pl.extend(frontier_abi::presets::RULESET_HASH);
    pl.extend(g.p.quicknet_pk_hash);
    pl.extend(g.p.reveal_window.to_le_bytes());
    pl.extend(g.p.seed_margin.to_le_bytes());
    pl.extend(g.p.r_max.to_le_bytes());
    pl.extend(g.p.program_version.to_le_bytes());
    let body = g.rec(
        Kind::SEASON_CREATED,
        NB,
        &8u64.to_le_bytes(),
        &pl,
        &[(En::Season, season)],
    );
    let payout = g.payout.clone();
    let ixs = [ix::create_season(&g.a, auth, &sp, &payout)];
    g.send(&ixs, &[&authority], &OP, vec![body]);
    for r in 0..16u8 {
        let k = g.ctx.beacon_log(r);
        g.make(k, AccountKind::BeaconLog);
        g.d(&k)[BL::REGION] = r;
    }
    let ixs = [ix::init_beacon_logs(&g.a, auth)];
    g.send(&ixs, &[&authority], &OP, vec![]);
    for f in 0..6u8 {
        for s in 0..JS::SHARDS_PER_FACTION {
            let k = g.ctx.join_shard(f, s);
            g.make(k, AccountKind::JoinShard);
            g.d(&k)[JS::FACTION] = f;
            g.d(&k)[JS::SHARD] = s;
        }
        let ixs = [ix::init_shards(&g.a, auth, f)];
        g.send(&ixs, &[&authority], &OP, vec![]);
    }
    let keeper = Keypair::new_from_array(KEEPER_SEED);
    let kpk = keeper.pubkey();
    g.at(g.round_time(gr) + 3);
    let arg = g.beacon(gr);
    let gseed = fclient::beacon::seed_of(
        gr,
        &fclient::beacon::decompress_sig(&arg.sig48).expect("sig"),
    );
    g.put(&season, S::GENESIS_SEED, &gseed);
    g.d(&season)[S::STATUS] = 3;
    let mut pl = gr.to_le_bytes().to_vec();
    pl.extend(gseed);
    let body = g.rec(
        Kind::GENESIS_SEED,
        NB,
        &8u64.to_le_bytes(),
        &pl,
        &[(En::Season, season)],
    );
    let ixs = [ix::consume_genesis_seed(&g.a, kpk, &arg)];
    g.send(&ixs, &[&keeper], &OP, vec![body]);
    // ---- genesis rings 0..=3 and the provinces of one region
    let mut ring_seeds = BTreeMap::new();
    let fk = g.ctx.frontier();
    for d in 0..=3u16 {
        let seed = sha256(&[RS::GENESIS_RING_DOMAIN, &gseed, &d.to_le_bytes()]);
        let rk = g.ctx.ring_seed(d);
        g.make(rk, AccountKind::RingSeed);
        g.put(&rk, RS::D, &d.to_le_bytes());
        g.d(&rk)[RS::STATUS] = RS::STATUS_SEEDED;
        g.put(&rk, RS::SEED, &seed);
        g.put(&fk, FR::RINGS_OPENED, &(d + 1).to_le_bytes());
        let mut pl = (g.time + 1).to_le_bytes().to_vec();
        pl.extend(0u64.to_le_bytes());
        pl.extend(seed);
        let body = g.rec(
            Kind::RING_OPEN,
            NB,
            &d.to_le_bytes(),
            &pl,
            &[(En::Frontier, fk)],
        );
        let ixs = [ix::open_ring(&g.a, kpk, d)];
        g.send(&ixs, &[&keeper], &OP, vec![body]);
        ring_seeds.insert(d, seed);
    }
    let mut by_region: BTreeMap<u8, Vec<ProvinceCoord>> = BTreeMap::new();
    for d in 2..=3u32 {
        for pc in ring_provinces(d) {
            let t = generate_province(&ring_seeds[&(d as u16)], pc);
            if t.site_count >= 2 {
                by_region.entry(region_of(pc)).or_default().push(pc);
            }
        }
    }
    let (region, pcs) = by_region
        .into_iter()
        .max_by_key(|(r, v)| (v.len(), *r))
        .expect("a region");
    let prov: Vec<(i32, i32)> = pcs.iter().take(5).map(|c| (c.p, c.q)).collect();
    assert_eq!(
        prov.len(),
        5,
        "five provinces with two sites in region {region}"
    );
    let mut next = BTreeMap::new();
    for &(p, q) in &prov {
        let pc = ProvinceCoord::new(p, q);
        let ring = pc.ring() as u16;
        let seed = ring_seeds[&ring];
        let t = generate_province(&seed, pc);
        let pk = g.ctx.province(p, q);
        g.make(pk, AccountKind::Province);
        encode_terrain(&t, g.d(&pk));
        let wedge = pc.wedge().unwrap_or(6);
        g.put(&pk, PV::P, &i16b(p));
        g.put(&pk, PV::Q, &i16b(q));
        g.put(&pk, PV::RING, &ring.to_le_bytes());
        g.d(&pk)[PV::WEDGE] = wedge;
        g.d(&pk)[PV::REGION] = region;
        for i in 0..PV::SITES_N {
            let o = PV::site(i);
            g.d(&pk)[o + SM::FACTION] = 6;
            g.put(&pk, o + SM::PEND0_BELL, &SM::NO_BELL.to_le_bytes());
            g.put(&pk, o + SM::PEND1_BELL, &SM::NO_BELL.to_le_bytes());
            g.put(&pk, o + SM::WALL_ITEM0_BELL, &SM::NO_BELL.to_le_bytes());
            g.put(&pk, o + SM::WALL_ITEM1_BELL, &SM::NO_BELL.to_le_bytes());
        }
        let cmp = camp::place(&seed, pc, &t, 0, false, true);
        if let Some(c) = cmp {
            g.d(&pk)[PV::CAMP + CP::TILE] = c.tile;
            g.d(&pk)[PV::CAMP + CP::STATE] = CP::STATE_PRESENT;
            g.put(&pk, PV::CAMP + CP::TROOPS, &c.troops.to_le_bytes());
            g.put(&pk, PV::CAMP + CP::GEN, &1u32.to_le_bytes());
        }
        g.put(&pk, PV::CAMP + CP::NEXT_CHECK_DAY, &1u32.to_le_bytes());
        let (ct, cn) = cmp.map_or((0xFF, 0), |c| (c.tile, c.troops));
        let mut pl = ring.to_le_bytes().to_vec();
        pl.extend([wedge, region]);
        pl.extend(terrain_digest(&t));
        pl.extend([t.site_count, ct]);
        pl.extend(cn.to_le_bytes());
        pl.push((ring < camp::CAMP_FIRST_RING as u16) as u8);
        let key = [p.to_le_bytes(), q.to_le_bytes()].concat();
        let body = g.rec(Kind::PROVINCE_OPEN, NB, &key, &pl, &[(En::Province, pk)]);
        let ixs = [ix::open_province(&g.a, kpk, p as i16, q as i16)];
        g.send(&ixs, &[&keeper], &OP, vec![body]);
        next.insert((p, q), 0u32);
    }
    let mut s = St {
        g,
        region,
        prov: prov.clone(),
        ring_seeds,
        cits: vec![],
        marches: vec![],
        anchors: BTreeMap::new(),
        seeds: BTreeMap::new(),
        want_seed: BTreeSet::new(),
        next_anchor: 0,
        anchor_upto: 20,
        next,
        arrivals: BTreeMap::new(),
        slots: BTreeMap::new(),
        keeper_budget_price: 0,
    };
    let _ = s.keeper_budget_price;
    // ---- citizens, one ticket cohort (bell 0), holdings (bell 2)
    let (a_, b_, c_, d_, e_) = (prov[0], prov[1], prov[2], prov[3], prov[4]);
    let plan: [(u8, (i32, i32), u8); 10] = [
        (0, a_, 0),
        (0, a_, 1),
        (1, b_, 0),
        (1, b_, 1),
        (2, c_, 0),
        (2, c_, 1),
        (2, d_, 0),
        (2, d_, 1),
        (2, e_, 0),
        (2, e_, 1),
    ];
    let t = s.g.genesis_ts + 30;
    s.advance_to(t);
    for (i, &(f, pq, site)) in plan.iter().enumerate() {
        let seed = [0x60 + i as u8; 32];
        let kp = Keypair::new_from_array(seed);
        let wallet = kp.pubkey().to_bytes();
        let tag15 = fclient::addr::citizen_tag15(&wallet);
        let addr = s.g.ctx.citizen(&wallet);
        let tag8 = u64::from_le_bytes(addr[..8].try_into().expect("8"));
        let shard = frontier_abi::addr::join_shard_of(&wallet);
        s.g.make(addr, AccountKind::Citizen);
        {
            let g = &mut s.g;
            g.put(&addr, C::WALLET, &wallet);
            g.d(&addr)[C::FACTION] = f;
            g.d(&addr)[C::FLAGS] = C::FLAG_JOINED;
            g.d(&addr)[C::EXPLORES_FLOOR_LEFT] = C::EXPLORES_FLOOR;
            g.d(&addr)[C::JOIN_SHARD] = shard;
            g.put(&addr, C::CITIZEN_TAG, &tag8.to_le_bytes());
            g.put(&addr, C::TICKET_BELL, &C::NO_TICKET.to_le_bytes());
        }
        let js = s.g.ctx.join_shard(f, shard);
        let mut pl = wallet.to_vec();
        pl.extend([f, shard]);
        pl.extend([0u8; 32]);
        pl.extend(0i64.to_le_bytes());
        let bell = s.g.bell();
        let body = s.g.rec(
            Kind::JOIN,
            bell,
            &tag15,
            &pl,
            &[(En::JoinShard, js), (En::Citizen, addr)],
        );
        let relay = s.relay_kp();
        let ixs = [ix::join(
            &s.g.a,
            kp.pubkey(),
            relay.pubkey(),
            f,
            &Address::default(),
            0,
            None,
        )];
        s.g.send(&ixs, &[&relay, &kp], &PLAYER, vec![body]);
        s.cits.push(Cit {
            seed,
            kp,
            addr,
            tag15,
            tag8,
            faction: f,
            shard,
            site: (pq.0, pq.1, site),
            gen: 0,
            seq: 0,
        });
    }
    let tb = s.g.bell();
    for c in 0..s.cits.len() {
        let (p, q, site) = s.cits[c].site;
        let pk = s.pk((p, q));
        let addr = s.cits[c].addr;
        let relay = s.g.relay.pubkey();
        // The cohort record of (P, Q, tb).
        let ci = (0..PV::COHORTS_N)
            .find(|&i| {
                let o = PV::cohort(i);
                let d = s.g.get(&pk);
                le(&d[o + CO::BELL..o + CO::BELL + 4]) as u32 == tb
                    && le(&d[o + CO::FILED..o + CO::FILED + 2]) > 0
            })
            .or_else(|| {
                (0..PV::COHORTS_N).find(|&i| {
                    le(&s.g.get(&pk)[PV::cohort(i) + CO::FILED..PV::cohort(i) + CO::FILED + 2]) == 0
                })
            })
            .expect("cohort record");
        let o = PV::cohort(ci);
        let filed = le(&s.g.get(&pk)[o + CO::FILED..o + CO::FILED + 2]) as u16 + 1;
        s.g.put(&pk, o + CO::BELL, &tb.to_le_bytes());
        s.g.put(&pk, o + CO::FILED, &filed.to_le_bytes());
        let escrow = frontier_abi::layout::rent(frontier_abi::layout::player::holding::SIZE);
        {
            let g = &mut s.g;
            g.put(&addr, C::TICKET_BELL, &tb.to_le_bytes());
            let st = C::TICKET_SITES;
            g.put(&addr, st, &i16b(p));
            g.put(&addr, st + 2, &i16b(q));
            g.d(&addr)[st + 4] = site;
            g.put(&addr, C::TICKET_ESCROW, &escrow.to_le_bytes());
            g.put(&addr, C::TICKET_FUNDER, &relay.to_bytes());
        }
        let mut sites = vec![0u8; 15];
        sites[..2].copy_from_slice(&i16b(p));
        sites[2..4].copy_from_slice(&i16b(q));
        sites[4] = site;
        let mut pl = tb.to_le_bytes().to_vec();
        pl.push(1);
        pl.extend(&sites);
        pl.extend(escrow.to_le_bytes());
        pl.extend(relay.to_bytes());
        let tag15 = s.cits[c].tag15;
        let body = s.g.rec(
            Kind::TICKET,
            tb,
            &tag15,
            &pl,
            &[(En::Citizen, addr), (En::Province, pk)],
        );
        let ixs = [ix::file_ticket(
            &s.g.a,
            &s.player(c),
            &[Site {
                p: p as i16,
                q: q as i16,
                site,
            }],
        )];
        let (r, w) = s.signers(c);
        s.g.send(&ixs, &[&r, &w], &PLAYER, vec![body]);
    }
    s.seed_now(tb);
    let (seed0, round0) = s.seeds[&tb];
    let final_ts = s.g.round_time(round0) + 600;
    for c in 0..s.cits.len() {
        let (p, q, site) = s.cits[c].site;
        let pk = s.pk((p, q));
        let hk = s.g.ctx.holding(p, q, site);
        let addr = s.cits[c].addr;
        let tag8 = s.cits[c].tag8;
        let score = fclient::land::ticket_score(&seed0, p, q, site, tag8);
        let gen = s.g.get(&pk)[PV::site(site as usize) + SM::GEN] + 1;
        s.cits[c].gen = gen;
        let relay = s.g.relay.pubkey();
        s.g.make(hk, AccountKind::Holding);
        {
            let g = &mut s.g;
            g.put(&hk, H::P, &i16b(p));
            g.put(&hk, H::Q, &i16b(q));
            g.d(&hk)[H::SITE] = site;
            g.d(&hk)[H::GEN] = gen;
            g.d(&hk)[H::STATE] = H::STATE_PROVISIONAL;
            g.put(&hk, H::OWNER_CITIZEN, &addr);
            g.put(&hk, H::TICKET_SCORE, &score.to_le_bytes());
            g.d(&hk)[H::FACTION] = s.cits[c].faction;
            g.put(&hk, H::TICKET_BELL, &tb.to_le_bytes());
            g.put(&hk, H::RENT_PAYER, &relay.to_bytes());
            g.put(&hk, H::FINAL_TS, &final_ts.to_le_bytes());
            let o = PV::site(site as usize);
            g.d(&pk)[o + SM::STATE] = SM::STATE_HOLDING;
            g.d(&pk)[o + SM::FACTION] = s.cits[c].faction;
            g.d(&pk)[o + SM::GEN] = gen;
            g.put(&addr, C::TICKET_BELL, &C::NO_TICKET.to_le_bytes());
            g.d(&addr)[C::HOLDINGS_N] = 1;
        }
        let ci = (0..PV::COHORTS_N)
            .find(|&i| {
                le(&s.g.get(&pk)[PV::cohort(i) + CO::BELL..PV::cohort(i) + CO::BELL + 4]) as u32
                    == tb
            })
            .expect("cohort");
        let o = PV::cohort(ci) + CO::SETTLED;
        let settled = le(&s.g.get(&pk)[o..o + 2]) as u16 + 1;
        s.g.put(&pk, o, &settled.to_le_bytes());
        let js = s.g.ctx.join_shard(s.cits[c].faction, s.cits[c].shard);
        let mut pl = vec![settle_outcome::FRESH];
        pl.extend(tag8.to_le_bytes());
        pl.extend(score.to_le_bytes());
        pl.extend(0u64.to_le_bytes());
        pl.push(gen);
        pl.extend(final_ts.to_le_bytes());
        pl.extend(tb.to_le_bytes());
        let key = [p.to_le_bytes().as_slice(), &q.to_le_bytes(), &[site]].concat();
        let bell = s.g.bell();
        let body = s.g.rec(
            Kind::SETTLE,
            bell,
            &key,
            &pl,
            &[
                (En::JoinShard, js),
                (En::Citizen, addr),
                (En::Holding, hk),
                (En::Province, pk),
            ],
        );
        let st = Site {
            p: p as i16,
            q: q as i16,
            site,
        };
        let wallet = s.cits[c].kp.pubkey();
        let ixs = [ix::settle_ticket(
            &s.g.a,
            kpk,
            &wallet,
            s.cits[c].faction,
            0,
            st,
            tb,
            &[st],
            SeedSource::Cache { nonce: 0 },
            None,
        )];
        s.g.send(&ixs, &[&keeper], &OP, vec![body]);
    }
    // ---- bell 4: musters (every province resolved through bell 2)
    let t = s.g.bell_start(4) + 60;
    s.advance_to(t);
    for &pq in &prov {
        s.resolve_through(pq, 2);
    }
    let t = (s.g.bell_start(4) + 100).max(s.g.time + 1);
    s.advance_to(t);
    let (ta, tb_, tc, td, te) = (
        s.tiles(a_),
        s.tiles(b_),
        s.tiles(c_),
        s.tiles(d_),
        s.tiles(e_),
    );
    let h0 = s.muster(0, 800, ta[0]);
    let h9 = s.muster(0, 200, ta[1]);
    let h1 = s.muster(1, 300, ta[2]);
    let h2 = s.muster(2, 600, tb_[0]);
    let h10 = s.muster(2, 250, tb_[1]);
    let h3 = s.muster(3, 500, tb_[2]);
    let w1 = s.muster(4, 900, tc[0]);
    let x1 = s.muster(4, 150, tc[1]);
    let w2 = s.muster(5, 800, tc[2]);
    let w3 = s.muster(6, 700, td[0]);
    let w4 = s.muster(7, 650, td[1]);
    let w5 = s.muster(8, 1000, te[0]);
    let w6 = s.muster(9, 600, te[1]);
    let _ = h2;
    // ---- bell 6: departures and an explore
    let t = s.g.bell_start(6) + 60;
    s.advance_to(t);
    for &pq in &prov {
        s.resolve_through(pq, 4);
    }
    let t = (s.g.bell_start(6) + 100).max(s.g.time + 1);
    s.advance_to(t);
    let tip_min = fees::min_tip_lamports(
        s.g.p.min_reveal_priority_milli,
        s.g.p.reveal_cu_limit,
        s.g.p.reveal_loaded_limit,
    );
    let tip = 2 * tip_min;
    let (a8, b8) = (s.tiles(a_), s.tiles(b_));
    let contested_a = a8[5];
    let m_h0 = s.depart(0, h0, 8, b_, tb_[0], 1, tip, SealKind::Valid);
    let m_h1 = s.depart(1, h1, 8, b_, b8[6], 0, tip_min, SealKind::Valid);
    let m_h9 = s.depart(0, h9, 9, b_, b8[7], 0, tip, SealKind::Garbage);
    let m_h3 = s.depart(3, h3, 8, a_, a8[12], 1, tip, SealKind::Garbage);
    let m_h10 = s.depart(2, h10, 9, a_, a8[6], 0, tip, SealKind::BadPlain);
    let m_w1 = s.depart(4, w1, 8, a_, contested_a, 0, tip, SealKind::Valid);
    let m_w2 = s.depart(5, w2, 8, a_, a8[7], 2, tip, SealKind::Valid);
    let m_w3 = s.depart(6, w3, 8, a_, a8[8], 0, tip, SealKind::Valid);
    let m_w4 = s.depart(7, w4, 8, a_, a8[9], 0, tip, SealKind::Valid);
    let m_w5 = s.depart(8, w5, 8, a_, a8[10], 3, tip, SealKind::Valid);
    let m_w6 = s.depart(9, w6, 8, a_, a8[11], 0, tip, SealKind::Valid);
    // The explore of x1 (C4's second host) over two of C's tiles.
    let (cp, cq) = c_;
    let pkc = s.pk(c_);
    let hk4 = s.g.ctx.holding_of_host(x1).expect("holding");
    let tiles = [tc[3], tc[4]];
    let eb = s.g.bell();
    {
        let g = &mut s.g;
        let e = H::EXPLORE;
        g.put(&hk4, e + X::BELL, &eb.to_le_bytes());
        g.put(&hk4, e + X::P, &i16b(cp));
        g.put(&hk4, e + X::Q, &i16b(cq));
        g.put(&hk4, e + X::TILES, &tiles);
        g.put(&hk4, e + X::HOST, &x1.to_le_bytes());
        g.d(&hk4)[e + X::STATE] = X::STATE_PENDING;
        let em = le(&g.get(&pkc)[PV::EXPLORED_MASK..PV::EXPLORED_MASK + 8])
            | 1 << tiles[0]
            | 1 << tiles[1];
        g.put(&pkc, PV::EXPLORED_MASK, &em.to_le_bytes());
    }
    let mut pl = cp.to_le_bytes().to_vec();
    pl.extend(cq.to_le_bytes());
    pl.push(2);
    pl.extend(tiles);
    let c4 = s.cits[4].addr;
    let body = s.g.rec(
        Kind::EXPLORE,
        eb,
        &x1.to_le_bytes(),
        &pl,
        &[(En::Citizen, c4), (En::Holding, hk4), (En::Province, pkc)],
    );
    let ixs = [ix::explore(
        &s.g.a,
        &s.player(4),
        s.href(4),
        (cp as i16, cq as i16),
        x1,
        &tiles,
    )];
    let (r, w) = s.signers(4);
    s.g.send(&ixs, &[&r, &w], &PLAYER, vec![body]);
    s.want_seed.insert(eb);
    // ---- bell 8: origins resolve bell 6 (B late), departures settle, the
    // owner reveals h3 in-bell, the explore settles.
    let t = s.g.bell_start(8) + 60;
    s.advance_to(t);
    for &pq in &[a_, c_, d_, e_] {
        s.resolve_through(pq, 6);
    }
    for &m in &[m_h0, m_h1, m_h9, m_w1, m_w2, m_w3, m_w4, m_w5, m_w6] {
        s.settle_departure(m);
    }
    s.reveal(m_h3, true, 0);
    s.seed_now(eb);
    let (seed6, _) = s.seeds[&eb];
    let mut floor = C::EXPLORES_FLOOR;
    let mut per = [0u32; 2];
    let mut used = 0u8;
    for (i, &tl) in tiles.iter().enumerate() {
        let fl = floor > 0;
        if fl {
            floor -= 1;
            used += 1;
        }
        per[i] = explore::roll(&seed6, ProvinceCoord::new(cp, cq), tl, x1, fl).works;
    }
    {
        let g = &mut s.g;
        g.d(&c4)[C::EXPLORES_FLOOR_LEFT] = floor;
        g.put(&hk4, H::EXPLORE, &[0u8; X::SIZE]);
    }
    let mut pl = per[0].to_le_bytes().to_vec();
    pl.extend(per[1].to_le_bytes());
    pl.extend((per[0] + per[1]).to_le_bytes());
    pl.push(used);
    let bell = s.g.bell();
    let body = s.g.rec(
        Kind::EXPLORE_RESULT,
        bell,
        &x1.to_le_bytes(),
        &pl,
        &[(En::Citizen, c4), (En::Holding, hk4)],
    );
    let wallet4 = s.cits[4].kp.pubkey();
    let ixs = [ix::settle_explore(
        &s.g.a,
        kpk,
        s.href(4),
        &wallet4,
        eb,
        region,
        SeedSource::Cache { nonce: 0 },
    )];
    s.g.send(&ixs, &[&keeper], &OP, vec![body]);
    // ---- bell 9: the window of bell 8 (keeper reveals), B's late resolve
    let a8t = s.g.bell_end(8) + 2;
    s.advance_to(a8t + 30);
    s.reveal(m_h0, false, 0);
    for &m in &[m_w1, m_w2, m_w3, m_w4] {
        s.reveal(m, false, 0);
    }
    let t = a8t + 150;
    s.advance_to(t);
    s.reveal(m_w5, false, 1_000_000); // late (≥ lateness_slots) and priced
    assert!(!s.reveal(m_w6, false, 0), "the sixth f2 arrival is refused");
    let t = s.g.bell_start(9) + 400;
    s.advance_to(t);
    s.resolve_through(b_, 7); // the lagging origin
    for &m in &[m_h3, m_h10] {
        s.settle_departure(m);
    }
    // ---- bell 10: gathers and resolves of bell 8, the rest resolves
    let t = s.g.bell_start(10) + 30;
    s.advance_to(t);
    for &pq in &prov {
        s.resolve_through(pq, 9);
    }
    // ---- bell 11: a defence claim, settlements
    let t = s.g.bell_start(11) + 30;
    s.advance_to(t);
    let (slot_i, _) = s.marches[m_w5].slot.expect("w5 holds a slot");
    let sk = s.g.ctx.arrival_slot(a_.0, a_.1, 8, 2, slot_i);
    let ev = AS::evidence(s.g.get(&sk)).expect("evidence");
    let dparams = DefenceParams {
        defence_cap_milli: s.g.p.defence_cap_milli,
        tip_min,
    };
    let refund = fees::defence_refund(&ev, &dparams).min(s.g.p.per_bell_region_cap);
    assert!(refund > 0, "the late priced reveal earns a refund");
    let day = day_of(8);
    let ck = s.g.ctx.defence_claim(&kpk.to_bytes(), day);
    s.g.make(ck, AccountKind::DefenceClaim);
    s.g.put(&ck, DC::BENEFICIARY, &kpk.to_bytes());
    s.g.put(&ck, DC::DAY, &day.to_le_bytes());
    s.g.put(&ck, DC::CLAIMED, &refund.to_le_bytes());
    s.g.put(&ck, DC::COUNT, &1u32.to_le_bytes());
    s.g.d(&sk)[AS::CLAIMED] = 1;
    if let Some(a) = s.g.acc.get_mut(&dp) {
        a.lamports -= refund;
    }
    let mut pl = day.to_le_bytes().to_vec();
    pl.push(1);
    pl.extend(refund.to_le_bytes());
    pl.push(0);
    let bell = s.g.bell();
    let body =
        s.g.rec(Kind::DEFENCE_CLAIM, bell, &kpk.to_bytes(), &pl, &[]);
    let ixs = [ix::claim_defence(
        &s.g.a,
        kpk,
        day,
        &[ClaimSlot {
            p: a_.0,
            q: a_.1,
            bell: 8,
            faction: 2,
            i: slot_i,
        }],
    )];
    s.g.send(&ixs, &[&keeper], &OP, vec![body]);
    for &m in &[m_h0, m_h1, m_h3, m_w1, m_w2, m_w3, m_w4, m_w5, m_w6] {
        s.settle_transit(m, false);
    }
    // ---- bell 12: the bad plaintext settles; C6 musters a second host in D
    // that departs at bell 14 and is still marching when the archive ends.
    let t = s.g.bell_start(12) + 30;
    s.advance_to(t);
    s.settle_transit(m_h10, false);
    s.resolve_through(d_, 10);
    let h11 = s.muster(6, 120, td[2]);
    let t = s.g.bell_start(14) + 60;
    s.advance_to(t);
    s.resolve_through(d_, 12);
    let t = (s.g.bell_start(14) + 100).max(s.g.time + 1);
    s.advance_to(t);
    let m_h11 = s.depart(6, h11, 20, c_, tc[5], 0, tip, SealKind::Valid);
    let _ = m_h11;
    // ---- two days later: archive bells 0..=12, close anchors, caches and
    // settled slots; the garbage seal of h9 settles from the archive.
    for b in 0..=12u32 {
        if !s.seeds.contains_key(&b) {
            s.want_seed.insert(b);
        }
    }
    let t = s.g.time + 1;
    s.advance_to(t);
    s.seed_now(12);
    let last_a = s.anchors[&12].0;
    let t = last_a + s.g.p.archive_after as i64 + 60;
    s.advance_to(t);
    let arch = s.g.ctx.anchor_archive(region, AA::part_of(0));
    s.g.make(arch, AccountKind::AnchorArchive);
    s.g.d(&arch)[AA::REGION] = region;
    s.g.put(&arch, AA::PART, &0u32.to_le_bytes());
    s.g.put(&arch, AA::RENT_TO, &kpk.to_bytes());
    let bells: Vec<u32> = (0..=12).collect();
    for chunk in bells.chunks(8) {
        let mut bodies = vec![];
        let mut items = vec![];
        for &b in chunk {
            let (a, _, round) = s.anchors[&b];
            let (seed, _) = s.seeds[&b];
            let off = (a - s.g.bell_end(b)) as u32;
            let sig = s.g.key.sign(round);
            let (tb_o, tb_m) = AA::bit(AA::TOMBSTONE, b);
            let (ar_o, ar_m) = AA::bit(AA::ARCHIVED, b);
            {
                let g = &mut s.g;
                g.d(&arch)[tb_o] |= tb_m;
                g.d(&arch)[ar_o] |= ar_m;
                let e = AA::entry(b);
                g.put(&arch, e + AE::A_OFF, &off.to_le_bytes());
                g.put(&arch, e + AE::SEED, &seed);
                g.put(&arch, e + AE::SIG, &sig);
            }
            let key = [[region].as_slice(), &0u32.to_le_bytes()].concat();
            let mut pl = b.to_le_bytes().to_vec();
            pl.extend(off.to_le_bytes());
            pl.extend(seed);
            let bell = s.g.bell();
            bodies.push(s.g.rec(Kind::ARCHIVE, bell, &key, &pl, &[]));
            let ak = s.g.ctx.bell_anchor(b, region);
            let raw = [b.to_le_bytes().as_slice(), &[region]].concat();
            bodies.push(s.g.close_rec(AccountKind::BellAnchor, &raw, &ak, &kpk, bell));
            items.push(ArchiveItem {
                bell: b,
                cache_nonce: 0,
                anchor_rent_to: kpk,
            });
        }
        let ixs = [ix::archive_anchors(&s.g.a, kpk, region, 0, &items)];
        s.g.send(&ixs, &[&keeper], &OP, bodies);
    }
    for b in 0..=12u32 {
        let k = s.g.ctx.seed_cache(b, region, 0);
        let raw = [b.to_le_bytes().as_slice(), &[region, 0]].concat();
        let bell = s.g.bell();
        let body = s.g.close_rec(AccountKind::SeedCache, &raw, &k, &kpk, bell);
        let ixs = [ix::close_seed_cache(&s.g.a, kpk, b, region, 0, kpk)];
        s.g.send(&ixs, &[&keeper], &OP, vec![body]);
    }
    s.settle_transit(m_h9, true);
    for m in [m_h0, m_w1, m_w2, m_w3, m_w5] {
        let Some((i, ben)) = s.marches[m].slot else {
            continue;
        };
        let (p, q) = s.marches[m].dest;
        let f = s.marches[m].faction;
        let k = s.g.ctx.arrival_slot(p, q, 8, f, i);
        if !s.g.has(&k) {
            continue;
        }
        let raw = fclient::addr::raw_arrival_slot(p, q, 8, f, i);
        let bell = s.g.bell();
        let body =
            s.g.close_rec(AccountKind::ArrivalSlot, &raw, &k, &ben, bell);
        let ixs = [ix::close_arrival_slot(
            &s.g.a, kpk, p as i16, q as i16, 8, f, i, ben, false,
        )];
        s.g.send(&ixs, &[&keeper], &OP, vec![body]);
    }
    let _ = (w4, w6, m_w6, &s.ring_seeds, &s.prov);
    s.g.input(
        "synthetic march season (verify_core::fixture::march): real transactions, PS2 records, chains, test-key signatures, stock tlock seals and the kernels; the clash write-back and the TRANSIT_SETTLED encoding are this module's (W4-D notes §3)",
        &[
            "lagging-origin",
            "low-tip-rout",
            "bad-seal-revealed-destroyed",
            "bad-seal-unrevealed-destroyed",
            "bad-plaintext-destroyed",
            "bad-seal-settled-after-archive",
            "slot-displacement",
            "quota-refused-arrival-bounced",
            "skip-quiet-runs",
            "archived-anchors-and-tombstones",
            "defence-claim",
            "explore",
            "contested-clash",
            "in-flight-march-at-the-end",
            "no-arrival-clash",
        ],
    )
}
