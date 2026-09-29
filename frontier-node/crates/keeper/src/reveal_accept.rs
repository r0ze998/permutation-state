//! The `/v1/reveal` accept path (M1 contract §8.2 loopback API, I-24, §5.11
//! Reveal checks 1–5). The sending pipeline is W4-C's; this module decides
//! whether owner reveal material is **accepted into the queue**, with the
//! answers the relay passes through to the browser:
//!
//! | answer | when |
//! |---|---|
//! | `202 {accepted, track}` | the material opens the march's seal root and the window is open (or not open yet) |
//! | `409 CommitMismatch` | `sha256(commit(plain, salt) ‖ ct_hash) ≠ transit.seal_root` |
//! | `409 TransitState` | no Holding at the address, or its transit slot is not in state 1–3 |
//! | `409 ArrivalBell` | the transit arrives at or after the season's `end_bell`, whatever the season status (no anchor of that bell can exist; W6T-2) |
//! | `409 Shielded` | §5.11 step 6's shield rules: the destination tile is another faction's holding site shielded past the arrival bell, or the host's own Holding is shielded at `bell_start(arrive)` (not dormant) and the destination is another faction's holding site (W6T-2; w6-s7 routed 27 honest marches this way while their owners were told 202) |
//! | `410 WindowClosed` | THE anchor's window closed (`now ≥ A + W` or the BeaconLog reached `S(A)`), the bell is archived, the latch is closed (ClashInputs present or the destination resolved past `arrive`), or the season no longer takes reveals |
//! | `422 BadPlaintext` | `Plain::validate` against the transit's host and arrival bell |
//!
//! The checks mirror the program's Reveal in its order (season, holding and
//! transit, plaintext, commitment, window, latch, the shield part of step
//! 6) and read the same accounts (canonical addresses only). The path and
//! travel-time part of step 6 stays the program's. Every refusal body
//! carries its code as `code` and as `error` (the relay's field). An anchor not yet posted is not a
//! refusal: owners submit from the arrival bell's start (§9.1) and the
//! keeper holds the material until THE anchor lands.

use std::future::Future;
use std::pin::Pin;

use solana_address::Address;

use fclient::abi::status;
use fclient::addr::{archive_part, Addresses};
use fclient::clock::SeasonClock;
use fclient::decode::{
    AnchorArchive, BeaconLog, BellAnchor, ClashInputs, Holding, Province, Season,
};
use fclient::ports::{Account, ChainPort};
use fclient::seal;

/// Reveal material as `/v1/reveal` receives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealReq {
    pub holding: Address,
    pub transit_slot: u8,
    pub plain: [u8; 37],
    pub salt: [u8; 32],
    pub ct_hash: [u8; 32],
}

/// What the accept path learned (queued with the material for W4-C).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub host_id: u64,
    pub faction: u8,
    pub arrive: u32,
    pub dest: (i16, i16),
    pub tile: u8,
    pub region: u8,
    pub commit: [u8; 32],
    /// THE anchor's `A + W(arrive)` when the anchor is posted.
    pub close: Option<i64>,
}

/// Why the material is refused: `(HTTP status, code, detail)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub http: u16,
    pub code: &'static str,
    pub detail: String,
}

fn refuse(http: u16, code: &'static str, detail: impl Into<String>) -> Refusal {
    Refusal {
        http,
        code,
        detail: detail.into(),
    }
}

/// The accounts the judgement reads (canonical addresses, `None` = absent
/// or not the program's).
#[derive(Clone, Debug, Default)]
pub struct Reads {
    pub season: Option<Account>,
    pub holding: Option<Account>,
    pub anchor: Option<Account>,
    pub archive: Option<Account>,
    pub beacon_log: Option<Account>,
    pub inputs: Option<Account>,
    pub dest_province: Option<Account>,
    /// The canonical address of the Holding read (from its stored P, Q,
    /// site); `None` = not checked (unit fixtures).
    pub canonical_holding: Option<Address>,
}

/// Step 1: the holding and its transit (needed to name the other accounts).
pub fn transit_of(req: &RevealReq, h: Option<&Account>) -> Result<(Holding, usize), Refusal> {
    let h = h
        .and_then(|a| Holding::decode(&a.data).ok())
        .ok_or_else(|| refuse(409, "TransitState", "no Holding at that address"))?;
    let i = req.transit_slot as usize;
    let tr = h
        .transit
        .get(i)
        .ok_or_else(|| refuse(400, "BadRequest", "transit_slot must be 0-3"))?;
    if !(1..=3).contains(&tr.state) {
        return Err(refuse(
            409,
            "TransitState",
            format!("transit slot {i} is in state {}", tr.state),
        ));
    }
    Ok((h, i))
}

/// The whole judgement over the accounts read at `now` (pure; the program's
/// Reveal checks 1–5 in order).
pub fn judge(req: &RevealReq, r: &Reads, now: i64) -> Result<Accepted, Refusal> {
    let s = r
        .season
        .as_ref()
        .and_then(|a| Season::decode(&a.data).ok())
        .ok_or_else(|| refuse(410, "WindowClosed", "no season"))?;
    // 1. The season takes reveals (the keeper prologue's status set),
    // before the holding (the program's order; wave-3 review, W3-C).
    let eff = s.effective_status(now);
    if eff != status::RUNNING && eff != status::ENDED {
        return Err(refuse(410, "WindowClosed", "the season takes no reveal"));
    }
    let (h, i) = transit_of(req, r.holding.as_ref())?;
    // The Holding of this season at its canonical address (the program's
    // step 2 refuses any other).
    if h.h.season_id != s.h.season_id || r.canonical_holding.is_some_and(|k| k != req.holding) {
        return Err(refuse(
            409,
            "TransitState",
            "not this season's Holding at its canonical address",
        ));
    }
    let tr = h.transit[i];
    // No anchor exists at or after the season's end, Running or Ended:
    // the march can never be revealed (W6T-2).
    if tr.arrive_bell >= s.end_bell {
        return Err(refuse(
            409,
            "ArrivalBell",
            format!(
                "arrival bell {} is at or after the season's end bell {}",
                tr.arrive_bell, s.end_bell
            ),
        ));
    }
    // 2. The plaintext against the transit.
    let p = seal::unpack(&req.plain);
    seal::validate(&p, tr.host_id, tr.arrive_bell)
        .map_err(|e| refuse(422, "BadPlaintext", format!("{e:?}")))?;
    // 3. The commitment opens the seal root.
    let commit = seal::commit(&req.plain, &req.salt);
    if seal::seal_root(&commit, &req.ct_hash) != tr.seal_root {
        return Err(refuse(
            409,
            "CommitMismatch",
            "commit ‖ ct_hash ≠ seal_root",
        ));
    }
    // 4. The window of THE anchor of (arrive, region(dest)).
    let (dp, dq) = (p.dest_p as i32, p.dest_q as i32);
    let region = fclient::ix::region_of(dp, dq);
    let sc = SeasonClock::from_season(&s);
    let arrive = tr.arrive_bell;
    let mut close = None;
    match r
        .anchor
        .as_ref()
        .and_then(|a| BellAnchor::decode(&a.data).ok())
    {
        Some(an) => {
            let latest = r
                .beacon_log
                .as_ref()
                .and_then(|a| BeaconLog::decode(&a.data).ok())
                .map_or(0, |b| b.latest_round);
            if !sc.reveal_open(arrive, an.a, now, latest) {
                return Err(refuse(
                    410,
                    "WindowClosed",
                    format!("bell {arrive}: the reveal window closed"),
                ));
            }
            close = Some(sc.reveal_close(arrive, an.a));
        }
        None => {
            let tomb = r
                .archive
                .as_ref()
                .and_then(|a| AnchorArchive::decode(&a.data).ok())
                .is_some_and(|x| x.tombstoned(arrive));
            if tomb {
                return Err(refuse(
                    410,
                    "WindowClosed",
                    format!("bell {arrive} is archived"),
                ));
            }
        }
    }
    // The destination Province must exist (the program reads it).
    let Some(dest_pv) = r
        .dest_province
        .as_ref()
        .and_then(|a| Province::decode(&a.data).ok())
    else {
        return Err(refuse(
            422,
            "BadPlaintext",
            format!("destination province ({dp}, {dq}) does not exist"),
        ));
    };
    // 5. The latch.
    if r.inputs
        .as_ref()
        .is_some_and(|a| ClashInputs::decode(&a.data).is_ok())
    {
        return Err(refuse(
            410,
            "WindowClosed",
            "LatchClosed: the province-bell is gathered",
        ));
    }
    if dest_pv.resolved_next > arrive {
        return Err(refuse(
            410,
            "WindowClosed",
            "LatchClosed: the destination resolved past the arrival bell",
        ));
    }
    // 6. The shield rules (the program's order: after the path, which
    // the program judges).
    let n = (dest_pv.site_count as usize).min(dest_pv.sites.len());
    if let Some(k) = (0..n).find(|&k| dest_pv.sites[k] == p.dest_tile) {
        let m = dest_pv.site_mirror[k];
        let other = m.state == fclient::abi::layout::site::STATE_HOLDING && m.faction != tr.faction;
        if other && m.shield_until_bell > arrive {
            return Err(refuse(
                409,
                "Shielded",
                format!(
                    "the destination holding is shielded until bell {}",
                    m.shield_until_bell
                ),
            ));
        }
        let dormant = h.flags & frontier_abi::layout::player::holding::FLAG_DORMANT_CACHE != 0;
        let start = permutation_rules::frontier::beacon::bell_start(s.genesis_ts, arrive);
        if other && !dormant && h.shield_until > start {
            return Err(refuse(
                409,
                "Shielded",
                "the host's own holding is shielded at the arrival bell: it may not target another faction's holding",
            ));
        }
    }
    Ok(Accepted {
        host_id: tr.host_id,
        faction: tr.faction,
        arrive,
        dest: (p.dest_p, p.dest_q),
        tile: p.dest_tile,
        region,
        commit,
        close,
    })
}

/// Reads the accounts of [`judge`] through a port and judges at the chain's
/// Clock.
pub async fn check<P: ChainPort>(
    port: &P,
    addrs: &Addresses,
    req: &RevealReq,
) -> Result<Accepted, Refusal> {
    let io = |e: fclient::ports::PortError| refuse(503, "ChainUnavailable", e.to_string());
    let clock = port.clock().await.map_err(io)?;
    let own = |a: Option<Account>| a.filter(|x| x.owner == addrs.program && !x.data.is_empty());
    let got = port
        .accounts(&[addrs.season, req.holding], 0)
        .await
        .map_err(io)?;
    let mut it = got.into_iter();
    let mut r = Reads {
        season: own(it.next().flatten()),
        holding: own(it.next().flatten()),
        ..Reads::default()
    };
    let (h, i) = transit_of(req, r.holding.as_ref())?;
    r.canonical_holding = Some(addrs.holding(h.p as i32, h.q as i32, h.site));
    let tr = h.transit[i];
    let p = seal::unpack(&req.plain);
    let (dp, dq) = (p.dest_p as i32, p.dest_q as i32);
    let region = fclient::ix::region_of(dp, dq);
    let arrive = tr.arrive_bell;
    let keys = [
        addrs.anchor(arrive, region),
        addrs.archive(region, archive_part(arrive)),
        addrs.beacon_log(region),
        addrs.clash_inputs(dp, dq, arrive),
        addrs.province(dp, dq),
    ];
    let got = port.accounts(&keys, 0).await.map_err(io)?;
    let mut it = got.into_iter();
    r.anchor = own(it.next().flatten());
    r.archive = own(it.next().flatten());
    r.beacon_log = own(it.next().flatten());
    r.inputs = own(it.next().flatten());
    r.dest_province = own(it.next().flatten());
    judge(req, &r, clock.unix_timestamp)
}

/// The API's view of the accept path (object-safe, so the router needs no
/// port type).
pub trait RevealGate: Send + Sync + 'static {
    fn check<'a>(
        &'a self,
        req: &'a RevealReq,
    ) -> Pin<Box<dyn Future<Output = Result<Accepted, Refusal>> + Send + 'a>>;
}

/// [`RevealGate`] over a chain port.
pub struct ChainGate<P: ChainPort + 'static> {
    pub port: P,
    pub addrs: Addresses,
}

impl<P: ChainPort + 'static> RevealGate for ChainGate<P> {
    fn check<'a>(
        &'a self,
        req: &'a RevealReq,
    ) -> Pin<Box<dyn Future<Output = Result<Accepted, Refusal>> + Send + 'a>> {
        Box::pin(check(&self.port, &self.addrs, req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::abi::layout as l;

    fn acct(data: Vec<u8>) -> Option<Account> {
        Some(Account {
            lamports: 1,
            data,
            owner: Address::new_from_array([1; 32]),
            executable: false,
        })
    }

    fn put(d: &mut [u8], o: usize, v: &[u8]) {
        d[o..o + v.len()].copy_from_slice(v);
    }

    /// A Running season, a holding with transit 1 → (3, -1) at bell 40.
    fn fixture() -> (RevealReq, Reads, i64) {
        let mut s = vec![0u8; fclient::abi::size::SEASON];
        put(&mut s, 0, fclient::abi::magic::SEASON);
        s[l::season::STATUS] = status::SEEDED;
        put(&mut s, l::season::GENESIS_TS, &1_000_000i64.to_le_bytes());
        put(&mut s, l::season::END_BELL, &1_008u32.to_le_bytes());
        put(&mut s, l::season::REVEAL_WINDOW, &600u32.to_le_bytes());
        put(&mut s, l::season::WINDOW_NEXT, &600u32.to_le_bytes());
        put(&mut s, l::season::WINDOW_FROM_BELL, &u32::MAX.to_le_bytes());
        put(&mut s, l::season::SEED_MARGIN, &60u32.to_le_bytes());
        put(&mut s, l::season::DRAND_GENESIS, &900_000i64.to_le_bytes());
        put(&mut s, l::season::DRAND_PERIOD, &3u32.to_le_bytes());
        let host = fclient::addr::host_id(2, 0, 1, 0, 7).unwrap();
        let plain = seal::Plain {
            version: 1,
            host_id: host,
            arrive_bell: 40,
            dest_p: 3,
            dest_q: -1,
            dest_tile: 5,
            stance: 1,
            ..Default::default()
        };
        let pt = seal::pack(&plain);
        let salt = [5u8; 32];
        let ct = [6u8; 32];
        let root = seal::seal_root(&seal::commit(&pt, &salt), &ct);
        let mut h = vec![0u8; fclient::abi::size::HOLDING];
        put(&mut h, 0, fclient::abi::magic::HOLDING);
        let o = l::holding::TRANSIT + l::holding::TRANSIT_STRIDE;
        h[o + l::transit::STATE] = 1;
        h[o + l::transit::FACTION] = 2;
        put(&mut h, o + l::transit::HOST_ID, &host.to_le_bytes());
        put(&mut h, o + l::transit::ARRIVE_BELL, &40u32.to_le_bytes());
        put(&mut h, o + l::transit::SEAL_ROOT, &root);
        let req = RevealReq {
            holding: Address::new_from_array([8; 32]),
            transit_slot: 1,
            plain: pt,
            salt,
            ct_hash: ct,
        };
        let mut pv = vec![0u8; fclient::abi::size::PROVINCE];
        put(&mut pv, 0, fclient::abi::magic::PROVINCE);
        let r = Reads {
            season: acct(s),
            holding: acct(h),
            dest_province: acct(pv),
            ..Reads::default()
        };
        (req, r, 1_000_000 + 40 * 600 + 10)
    }

    fn anchor(a: i64) -> Option<Account> {
        let mut d = vec![0u8; fclient::abi::size::BELL_ANCHOR];
        put(&mut d, 0, fclient::abi::magic::BELL_ANCHOR);
        put(&mut d, l::bell_anchor::BELL, &40u32.to_le_bytes());
        put(&mut d, l::bell_anchor::A, &a.to_le_bytes());
        acct(d)
    }

    #[test]
    fn the_accept_path_answers_like_the_program() {
        let (req, r, now) = fixture();
        let ok = judge(&req, &r, now).unwrap();
        assert_eq!(
            (ok.arrive, ok.dest, ok.faction, ok.close),
            (40, (3, -1), 2, None)
        );
        // Commitment.
        let mut bad = req.clone();
        bad.salt[0] ^= 1;
        assert_eq!(judge(&bad, &r, now).unwrap_err().code, "CommitMismatch");
        let mut bad = req.clone();
        bad.ct_hash[0] ^= 1;
        assert_eq!(judge(&bad, &r, now).unwrap_err().http, 409);
        // Plaintext against the transit (another arrival bell).
        let mut p = seal::unpack(&req.plain);
        p.arrive_bell = 41;
        let mut bad = req.clone();
        bad.plain = seal::pack(&p);
        let e = judge(&bad, &r, now).unwrap_err();
        assert_eq!((e.http, e.code), (422, "BadPlaintext"));
        // Transit slot empty.
        let mut bad = req.clone();
        bad.transit_slot = 0;
        assert_eq!(judge(&bad, &r, now).unwrap_err().code, "TransitState");
        // Anchor posted: open until A + W, then closed.
        let a = 1_000_000 + 41 * 600 + 15;
        let mut r2 = r.clone();
        r2.anchor = anchor(a);
        assert_eq!(judge(&req, &r2, a + 599).unwrap().close, Some(a + 600));
        let e = judge(&req, &r2, a + 600).unwrap_err();
        assert_eq!((e.http, e.code), (410, "WindowClosed"));
        // The BeaconLog reached S(A).
        let mut s =
            SeasonClock::from_season(&Season::decode(&r.season.clone().unwrap().data).unwrap());
        s.window_next = 600;
        let sr = s.seed_round(40, a);
        let mut bl = vec![0u8; fclient::abi::size::BEACON_LOG];
        put(&mut bl, 0, fclient::abi::magic::BEACON_LOG);
        put(&mut bl, l::beacon_log::LATEST_ROUND, &sr.to_le_bytes());
        r2.beacon_log = acct(bl);
        assert_eq!(judge(&req, &r2, a + 10).unwrap_err().http, 410);
        // Latch: inputs present, or the destination resolved past.
        let mut r3 = r.clone();
        let mut ci = vec![0u8; fclient::abi::size::CLASH_INPUTS];
        put(&mut ci, 0, fclient::abi::magic::CLASH_INPUTS);
        r3.inputs = acct(ci);
        assert!(judge(&req, &r3, now).unwrap_err().detail.contains("Latch"));
        let mut r4 = r.clone();
        let mut pv = vec![0u8; fclient::abi::size::PROVINCE];
        put(&mut pv, 0, fclient::abi::magic::PROVINCE);
        put(&mut pv, l::province::RESOLVED_NEXT, &41u32.to_le_bytes());
        r4.dest_province = acct(pv.clone());
        assert_eq!(judge(&req, &r4, now).unwrap_err().http, 410);
        put(&mut pv, l::province::RESOLVED_NEXT, &40u32.to_le_bytes());
        r4.dest_province = acct(pv);
        assert!(
            judge(&req, &r4, now).is_ok(),
            "resolved_next == arrive is open"
        );
        // Archived bell.
        let mut r5 = r.clone();
        let mut ar = vec![0u8; fclient::abi::size::ANCHOR_ARCHIVE];
        put(&mut ar, 0, fclient::abi::magic::ANCHOR_ARCHIVE);
        let k = 40; // bell 40 of part 0 (40 mod 72)
        ar[l::anchor_archive::TOMBSTONE + k / 8] |= 1 << (k % 8);
        r5.archive = acct(ar);
        assert!(judge(&req, &r5, now)
            .unwrap_err()
            .detail
            .contains("archived"));
        // An ended season still takes reveals for bells before the end.
        let mut r6 = r.clone();
        let mut sd = r6.season.clone().unwrap().data;
        sd[l::season::STATUS] = status::ENDED;
        r6.season = acct(sd.clone());
        assert!(judge(&req, &r6, now).is_ok());
        put(&mut sd, l::season::END_BELL, &40u32.to_le_bytes());
        r6.season = acct(sd);
        assert_eq!(judge(&req, &r6, now).unwrap_err().code, "ArrivalBell");
        // Wave-3 review: a Holding of another season, or read at a
        // non-canonical address; a destination that does not exist; the
        // season status comes before the holding.
        let mut r7 = r.clone();
        let mut hd = r7.holding.clone().unwrap().data;
        put(&mut hd, 8, &9u64.to_le_bytes());
        r7.holding = acct(hd);
        assert_eq!(judge(&req, &r7, now).unwrap_err().code, "TransitState");
        let mut r8 = r.clone();
        r8.canonical_holding = Some(Address::new_from_array([9; 32]));
        assert_eq!(judge(&req, &r8, now).unwrap_err().code, "TransitState");
        r8.canonical_holding = Some(req.holding);
        assert!(judge(&req, &r8, now).is_ok());
        let mut r9 = r.clone();
        r9.dest_province = None;
        assert_eq!(judge(&req, &r9, now).unwrap_err().http, 422);
        let mut r10 = r.clone();
        let mut sd = r10.season.clone().unwrap().data;
        sd[l::season::STATUS] = status::ABORTED;
        r10.season = acct(sd);
        r10.holding = None;
        assert_eq!(judge(&req, &r10, now).unwrap_err().code, "WindowClosed");
    }

    fn set_holding(r: &mut Reads, f: impl FnOnce(&mut Vec<u8>)) {
        let mut d = r.holding.clone().unwrap().data;
        f(&mut d);
        r.holding = acct(d);
    }

    /// §5.11 step 6 (w6-s7 criterion 4: 27 honest marches refused
    /// `Shielded` at every Reveal and routed, their owners told 202): the
    /// accept path answers 409 `Shielded` when the host's own Holding is
    /// shielded at `bell_start(arrive)` (not dormant) and the destination
    /// tile is another faction's holding site, or when that site is itself
    /// shielded past the arrival bell.
    #[test]
    fn reveal_accept_refuses_own_shield_war_target() {
        use l::site as sm;
        let (req, mut r, now) = fixture();
        let start = 1_000_000 + 40 * 600;
        let war = |shield_bell: u32| {
            acct(crate::testkit::province(
                0,
                &[
                    (5, sm::STATE_HOLDING, 1, shield_bell),
                    (9, sm::STATE_HOLDING, 2, 0),
                ],
            ))
        };
        let faction = |r: &mut Reads, f: u8| set_holding(r, |d| d[l::holding::FACTION] = f);
        faction(&mut r, 2);
        r.dest_province = war(0);
        assert!(
            judge(&req, &r, now).is_ok(),
            "no shield: a war target is open"
        );
        // Our holding shielded past the arrival bell's start.
        let shield = |r: &mut Reads, until: i64| {
            set_holding(r, |d| {
                d[l::holding::SHIELD_UNTIL..l::holding::SHIELD_UNTIL + 8]
                    .copy_from_slice(&until.to_le_bytes())
            })
        };
        shield(&mut r, start + 1);
        let e = judge(&req, &r, now).unwrap_err();
        assert_eq!((e.http, e.code), (409, "Shielded"), "{}", e.detail);
        // Dormant: the shield does not hold.
        set_holding(&mut r, |d| d[l::holding::FLAGS] |= 1);
        assert!(judge(&req, &r, now).is_ok(), "dormant");
        set_holding(&mut r, |d| d[l::holding::FLAGS] &= !1);
        // A shield that ends at the bell's start does not hold.
        shield(&mut r, start);
        assert!(judge(&req, &r, now).is_ok(), "shield over at bell_start");
        shield(&mut r, start + 1);
        // Our own faction's holding site, a free site, a tile with no site.
        for (tile, state, f) in [
            (5u8, sm::STATE_HOLDING, 2u8),
            (5, sm::STATE_FREE, 1),
            (6, sm::STATE_HOLDING, 1),
        ] {
            r.dest_province = acct(crate::testkit::province(0, &[(tile, state, f, 0)]));
            assert!(
                judge(&req, &r, now).is_ok(),
                "tile {tile} state {state} faction {f}"
            );
        }
        // The destination holding shielded past the arrival bell.
        shield(&mut r, 0);
        r.dest_province = war(41);
        let e = judge(&req, &r, now).unwrap_err();
        assert_eq!((e.http, e.code), (409, "Shielded"), "{}", e.detail);
        r.dest_province = war(40);
        assert!(
            judge(&req, &r, now).is_ok(),
            "shield until the arrival bell"
        );
    }

    /// An arrival at or after `end_bell` can never be revealed (no anchor
    /// of that bell exists; Reveal refuses `WrongStatus` once Ended): 409
    /// `ArrivalBell` whatever the season status, before the relay queues
    /// it (w6-s7: 9 such marches, 27 failed Reveals).
    #[test]
    fn reveal_accept_refuses_arrival_at_end_bell() {
        let (req, r, now) = fixture();
        for st in [status::RUNNING, status::ENDED] {
            for (end, ok) in [(40u32, false), (39, false), (41, true)] {
                let mut r2 = r.clone();
                let mut sd = r2.season.clone().unwrap().data;
                sd[l::season::STATUS] = st;
                put(&mut sd, l::season::END_BELL, &end.to_le_bytes());
                r2.season = acct(sd);
                match judge(&req, &r2, now) {
                    Ok(_) => assert!(ok, "status {st} end {end}: accepted"),
                    Err(e) => {
                        assert!(!ok, "status {st} end {end}: {e:?}");
                        assert_eq!(
                            (e.http, e.code),
                            (409, "ArrivalBell"),
                            "status {st} end {end}"
                        );
                    }
                }
            }
        }
    }
}
