//! Land rules the off-chain side needs (M1 contract §5.9, I-30, I-47, I-48):
//! the ticket score and its order, ticket expiry and cohorts, the ring
//! crowding rule, and the site a ticket settles next.
//!
//! These are **predictions**: the program decides. The keeper uses them to
//! order its SettleTicket burst (descending score, §8.2), to name the
//! displaced holder's accounts when a settlement will displace, and to
//! know when OpenRing(d > g) can succeed.
//!
//! The score is `rng::rand(S, "site", P ‖ Q ‖ site ‖ citizen_tag)` (§5.9)
//! with the §4.1 raw forms: P and Q as **i32** little-endian, site u8,
//! `citizen_tag` u64 little-endian. The contract names the fields but not
//! their widths; this is the key form every other land record uses
//! (SETTLE's key, `explore::roll`'s id). W3-C notes, dependency request D1:
//! the program (W3-A) and the verifier (V11) must use the same bytes, and
//! the function belongs in `permutation-rules::frontier` once W3-A pins it.

use crate::decode::{Citizen, Cohort, Frontier, Holding, Season};

/// Tickets expire `TICKET_COHORT_BELLS` bells after `ticket_bell` (I-47).
pub const TICKET_COHORT_BELLS: u32 = 24;
/// `Citizen.ticket_bell` when no ticket is open.
pub const NO_TICKET: u32 = u32::MAX;
/// Rent of a Province (`(128 + 4,096) × 5,080`, §4.2).
pub const PROVINCE_RENT: u64 = (128 + 4_096) * 5_080;
/// Rent of a ProvinceFund (`(128 + 128) × 5,080`).
pub const FUND_RENT: u64 = (128 + 128) * 5_080;

/// `rng::rand(S, "site", P ‖ Q ‖ site ‖ citizen_tag)`.
pub fn ticket_score(seed: &[u8; 32], p: i32, q: i32, site: u8, citizen_tag: u64) -> u64 {
    let mut id = [0u8; 17];
    id[..4].copy_from_slice(&p.to_le_bytes());
    id[4..8].copy_from_slice(&q.to_le_bytes());
    id[8] = site;
    id[9..17].copy_from_slice(&citizen_tag.to_le_bytes());
    permutation_rules::rng::rand(seed, b"site", &id)
}

/// Whether `(score_a, tag_a)` beats `(score_b, tag_b)`: the higher score
/// wins, a tie goes to the lower `citizen_tag` (§5.9).
pub fn beats(score_a: u64, tag_a: u64, score_b: u64, tag_b: u64) -> bool {
    score_a > score_b || (score_a == score_b && tag_a < tag_b)
}

/// A ticket's sites (the first `n`): `(P, Q, site)`. `n` is not stored in
/// the Citizen; unused entries are zero, and `(0, 0)` is the Concord (ring
/// 0, never a ticket site), so the first zero coordinate pair ends the list.
pub fn ticket_sites(c: &Citizen) -> Vec<(i16, i16, u8)> {
    c.ticket_sites
        .iter()
        .take_while(|s| (s.p, s.q) != (0, 0))
        .map(|s| (s.p, s.q, s.site))
        .collect()
}

/// Whether a ticket with `ticket_bell` is expired at `now_bell`.
pub fn ticket_expired(ticket_bell: u32, now_bell: u32) -> bool {
    ticket_bell != NO_TICKET && now_bell >= ticket_bell.saturating_add(TICKET_COHORT_BELLS)
}

/// A cohort record is free (reusable) when every ticket settled or its 24
/// bells passed (§5.9 note on cohorts).
pub fn cohort_free(c: &Cohort, now_bell: u32) -> bool {
    c.settled >= c.filed || now_bell >= c.bell.saturating_add(TICKET_COHORT_BELLS)
}

/// The province's cohort of `ticket_bell` is closed (a holding of it may
/// become final): every ticket settled, or 24 bells passed. A missing
/// record counts as closed (nothing filed there at that bell).
pub fn cohort_closed(cohorts: &[Cohort], ticket_bell: u32, now_bell: u32) -> bool {
    cohorts
        .iter()
        .filter(|c| c.filed > 0 && c.bell == ticket_bell)
        .all(|c| cohort_free(c, now_bell))
}

/// DECISIONS K9 (decided 2026-09-28, the candidate program rule): a
/// **fresh** settlement at a site of a Province waits while an **earlier**
/// ticket cohort of the same Province is still open (not every ticket
/// settled and fewer than 24 bells passed), so a later cohort can never
/// make an earlier winner `taken` and I-47's 24-bell bound holds. The
/// keeper does not send such a settlement (the program refuses or defers
/// it); displacement and `taken` are unaffected.
pub fn earlier_cohort_open(cohorts: &[Cohort], ticket_bell: u32, now_bell: u32) -> bool {
    cohorts
        .iter()
        .any(|c| c.filed > 0 && c.bell < ticket_bell && !cohort_free(c, now_bell))
}

/// What SettleTicket will do with a ticket's current site, as far as the
/// chain shows before it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteOutcome {
    /// Site free or released-free.
    Fresh,
    /// A provisional holding of the same cohort with a losing score.
    Displace {
        holder_citizen: solana_address::Address,
        holder_rent_payer: solana_address::Address,
        holder_score: u64,
    },
    /// Held (final, another cohort, or a better score): `ticket_next += 1`.
    Taken,
}

/// Predicts the outcome at a site whose mirror state is `mirror_state`
/// with `holding` its current Holding (if present).
pub fn predict_site(
    mirror_state: u8,
    holding: Option<&Holding>,
    ticket_bell: u32,
    score: u64,
    citizen_tag: u64,
    holder_tag: Option<u64>,
) -> SiteOutcome {
    use crate::abi::layout::{holding as hl, site};
    match mirror_state {
        site::STATE_FREE | site::STATE_RELEASED_FREE => SiteOutcome::Fresh,
        site::STATE_HOLDING => match (holding, holder_tag) {
            (Some(h), Some(ht))
                if h.state == hl::STATE_PROVISIONAL
                    && h.ticket_bell == ticket_bell
                    && beats(score, citizen_tag, h.ticket_score, ht) =>
            {
                SiteOutcome::Displace {
                    holder_citizen: h.owner_citizen,
                    holder_rent_payer: h.rent_payer,
                    holder_score: h.ticket_score,
                }
            }
            _ => SiteOutcome::Taken,
        },
        _ => SiteOutcome::Taken,
    }
}

/// θ in bps at `now` (§5.9 OpenRing: early before `theta_switch_secs`
/// since genesis, late after).
pub fn theta_bps(s: &Season, now: i64) -> u16 {
    if now.saturating_sub(s.genesis_ts) < s.theta_switch_secs as i64 {
        s.theta_early_bps
    } else {
        s.theta_late_bps
    }
}

/// Why OpenRing(d > g) cannot land yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RingWait {
    /// Not Running (genesis rings open while Seeded; crowding rings need Running).
    NotRunning,
    /// `d > r_max`.
    AtMax,
    /// Less than one bell since the last ring opened.
    TooSoon,
    /// No wedge has `wedge_occupied ≥ θ × wedge_open` on the folded values.
    NotCrowded,
    /// Wedge `w`'s fund cannot pay `d` provinces.
    Fund(u8),
}

/// The OpenRing rule for `d = frontier.rings_opened > g` (§5.9).
/// `fund_lamports[w]` are the six ProvinceFund balances.
pub fn ring_open_check(
    s: &Season,
    fr: &Frontier,
    fund_lamports: &[u64; 6],
    now: i64,
) -> Result<(), RingWait> {
    let d = fr.rings_opened;
    if d > s.r_max {
        return Err(RingWait::AtMax);
    }
    if s.effective_status(now) != crate::abi::status::RUNNING {
        return Err(RingWait::NotRunning);
    }
    let bell = crate::clock::bell_at(s.genesis_ts, now).unwrap_or(0);
    if bell < fr.last_ring_open_bell.saturating_add(1) {
        return Err(RingWait::TooSoon);
    }
    let theta = theta_bps(s, now) as u64;
    let crowded = (0..6).any(|w| {
        let open = fr.wedge_open[w] as u64;
        open > 0 && fr.wedge_occupied[w] as u64 * 10_000 >= theta * open
    });
    if !crowded {
        return Err(RingWait::NotCrowded);
    }
    for (w, &l) in fund_lamports.iter().enumerate() {
        if l.saturating_sub(FUND_RENT) < d as u64 * PROVINCE_RENT {
            return Err(RingWait::Fund(w as u8));
        }
    }
    Ok(())
}

/// Seconds a holding has been idle and whether it can be released (§5.9
/// ReleaseDormant: order 1, `now ≥ last_owner_action + release_after`, no
/// transit in state 1–3).
pub fn releasable(h: &Holding, s: &Season, now: i64) -> bool {
    h.order == 1
        && h.state != crate::abi::layout::holding::STATE_RELEASED
        && now
            >= h.last_owner_action
                .saturating_add(s.release_after_secs as i64)
        && !h.transit.iter().any(|t| (1..=3).contains(&t.state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_the_kernel_rand_over_the_raw_key() {
        let seed = [7u8; 32];
        let s = ticket_score(&seed, -3, 5, 11, 0x0102_0304_0506_0708);
        let mut id = vec![];
        id.extend_from_slice(&(-3i32).to_le_bytes());
        id.extend_from_slice(&5i32.to_le_bytes());
        id.push(11);
        id.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(s, permutation_rules::rng::rand(&seed, b"site", &id));
        // Sites and citizens draw independently.
        assert_ne!(s, ticket_score(&seed, -3, 5, 10, 0x0102_0304_0506_0708));
        assert_ne!(s, ticket_score(&seed, -3, 5, 11, 0x0102_0304_0506_0709));
    }

    #[test]
    fn ties_go_to_the_lower_tag() {
        assert!(beats(5, 9, 4, 1));
        assert!(!beats(4, 1, 5, 9));
        assert!(beats(5, 1, 5, 2));
        assert!(!beats(5, 2, 5, 1));
        assert!(!beats(5, 2, 5, 2), "a ticket never beats itself");
    }

    #[test]
    fn cohorts_close_when_settled_or_expired() {
        let c = |bell, filed, settled| Cohort {
            bell,
            filed,
            settled,
        };
        assert!(cohort_free(&c(10, 3, 3), 10));
        assert!(!cohort_free(&c(10, 3, 2), 33));
        assert!(cohort_free(&c(10, 3, 2), 34));
        let all = [c(10, 3, 2), c(11, 1, 1), Cohort::default()];
        assert!(!cohort_closed(&all, 10, 20));
        assert!(cohort_closed(&all, 11, 20));
        assert!(cohort_closed(&all, 12, 20), "nothing filed at 12");
        assert!(ticket_expired(10, 34) && !ticket_expired(10, 33));
        assert!(!ticket_expired(NO_TICKET, u32::MAX));
    }

    fn put(d: &mut [u8], o: usize, v: &[u8]) {
        d[o..o + v.len()].copy_from_slice(v);
    }

    fn season(genesis: i64) -> Season {
        use crate::abi::layout::season as l;
        let mut d = vec![0u8; crate::abi::size::SEASON];
        put(&mut d, 0, crate::abi::magic::SEASON);
        d[l::STATUS] = crate::abi::status::SEEDED;
        put(&mut d, l::GENESIS_TS, &genesis.to_le_bytes());
        put(&mut d, l::R_MAX, &16u16.to_le_bytes());
        put(&mut d, l::THETA_EARLY_BPS, &5_500u16.to_le_bytes());
        put(&mut d, l::THETA_LATE_BPS, &6_500u16.to_le_bytes());
        put(&mut d, l::THETA_SWITCH_SECS, &259_200u32.to_le_bytes());
        put(&mut d, l::RELEASE_AFTER_SECS, &864_000u32.to_le_bytes());
        Season::decode(&d).unwrap()
    }

    fn frontier(d: u16, last_bell: u32, open: [u32; 6], occ: [u32; 6]) -> Frontier {
        use crate::abi::layout::frontier as l;
        let mut b = vec![0u8; crate::abi::size::FRONTIER];
        put(&mut b, 0, crate::abi::magic::FRONTIER);
        put(&mut b, l::RINGS_OPENED, &d.to_le_bytes());
        put(&mut b, l::LAST_RING_OPEN_BELL, &last_bell.to_le_bytes());
        for w in 0..6 {
            put(&mut b, l::WEDGE_OPEN + 4 * w, &open[w].to_le_bytes());
            put(&mut b, l::WEDGE_OCCUPIED + 4 * w, &occ[w].to_le_bytes());
        }
        Frontier::decode(&b).unwrap()
    }

    #[test]
    fn the_crowding_rule() {
        let g = 1_000_000;
        let s = season(g);
        let rich = [10_000_000_000u64; 6];
        let now = g + 10 * 600;
        // 55 of 100 open in wedge 2 crosses θ early (55%).
        let fr = frontier(4, 3, [100; 6], [0, 0, 55, 0, 0, 0]);
        assert_eq!(ring_open_check(&s, &fr, &rich, now), Ok(()));
        let fr = frontier(4, 3, [100; 6], [0, 0, 54, 0, 0, 0]);
        assert_eq!(
            ring_open_check(&s, &fr, &rich, now),
            Err(RingWait::NotCrowded)
        );
        // Late θ (65%) after theta_switch_secs.
        let fr = frontier(4, 3, [100; 6], [0, 0, 60, 0, 0, 0]);
        assert_eq!(ring_open_check(&s, &fr, &rich, now), Ok(()));
        assert_eq!(
            ring_open_check(&s, &fr, &rich, g + 259_200),
            Err(RingWait::NotCrowded)
        );
        // One bell since the last opening; the funds; before genesis; r_max.
        let fr = frontier(4, 10, [100; 6], [0, 0, 60, 0, 0, 0]);
        assert_eq!(ring_open_check(&s, &fr, &rich, now), Err(RingWait::TooSoon));
        let fr = frontier(4, 3, [100; 6], [0, 0, 60, 0, 0, 0]);
        let mut poor = rich;
        poor[5] = FUND_RENT + 4 * PROVINCE_RENT - 1;
        assert_eq!(ring_open_check(&s, &fr, &poor, now), Err(RingWait::Fund(5)));
        poor[5] += 1;
        assert_eq!(ring_open_check(&s, &fr, &poor, now), Ok(()));
        assert_eq!(
            ring_open_check(&s, &fr, &rich, g - 1),
            Err(RingWait::NotRunning)
        );
        let fr = frontier(17, 3, [100; 6], [100; 6]);
        assert_eq!(ring_open_check(&s, &fr, &rich, now), Err(RingWait::AtMax));
        // A wedge with nothing open is never crowded.
        let fr = frontier(4, 3, [0; 6], [0; 6]);
        assert_eq!(
            ring_open_check(&s, &fr, &rich, now),
            Err(RingWait::NotCrowded)
        );
    }

    fn holding(state: u8, ticket_bell: u32, score: u64, owner: [u8; 32]) -> Holding {
        use crate::abi::layout::holding as l;
        let mut d = vec![0u8; crate::abi::size::HOLDING];
        put(&mut d, 0, crate::abi::magic::HOLDING);
        d[l::STATE] = state;
        d[l::ORDER] = 1;
        put(&mut d, l::OWNER_CITIZEN, &owner);
        put(&mut d, l::TICKET_SCORE, &score.to_le_bytes());
        put(&mut d, l::TICKET_BELL, &ticket_bell.to_le_bytes());
        put(&mut d, l::RENT_PAYER, &[9; 32]);
        Holding::decode(&d).unwrap()
    }

    #[test]
    fn site_outcomes() {
        use crate::abi::layout::site;
        assert_eq!(
            predict_site(site::STATE_FREE, None, 5, 1, 1, None),
            SiteOutcome::Fresh
        );
        assert_eq!(
            predict_site(site::STATE_RELEASED_FREE, None, 5, 1, 1, None),
            SiteOutcome::Fresh
        );
        assert_eq!(
            predict_site(site::STATE_RESERVED, None, 5, 1, 1, None),
            SiteOutcome::Taken
        );
        let h = holding(1, 5, 100, [3; 32]);
        match predict_site(site::STATE_HOLDING, Some(&h), 5, 200, 7, Some(8)) {
            SiteOutcome::Displace {
                holder_citizen,
                holder_rent_payer,
                holder_score,
            } => {
                assert_eq!(holder_citizen.to_bytes(), [3; 32]);
                assert_eq!(holder_rent_payer.to_bytes(), [9; 32]);
                assert_eq!(holder_score, 100);
            }
            o => panic!("{o:?}"),
        }
        // A lower score, another cohort, a final holding: taken.
        assert_eq!(
            predict_site(site::STATE_HOLDING, Some(&h), 5, 50, 7, Some(8)),
            SiteOutcome::Taken
        );
        assert_eq!(
            predict_site(site::STATE_HOLDING, Some(&h), 6, 200, 7, Some(8)),
            SiteOutcome::Taken
        );
        let f = holding(2, 5, 100, [3; 32]);
        assert_eq!(
            predict_site(site::STATE_HOLDING, Some(&f), 5, 200, 7, Some(8)),
            SiteOutcome::Taken
        );
        // Equal scores: the lower tag wins.
        assert!(matches!(
            predict_site(site::STATE_HOLDING, Some(&h), 5, 100, 7, Some(8)),
            SiteOutcome::Displace { .. }
        ));
        assert_eq!(
            predict_site(site::STATE_HOLDING, Some(&h), 5, 100, 9, Some(8)),
            SiteOutcome::Taken
        );
    }

    #[test]
    fn dormancy() {
        let s = season(0);
        let mut h = holding(2, 5, 1, [3; 32]);
        h.last_owner_action = 1_000;
        assert!(!releasable(&h, &s, 1_000 + 863_999));
        assert!(releasable(&h, &s, 1_000 + 864_000));
        h.transit[2].state = 1;
        assert!(!releasable(&h, &s, 2_000_000), "a transit in flight");
        h.transit[2].state = 0;
        h.order = 2;
        assert!(!releasable(&h, &s, 2_000_000), "order 1 only");
    }
    #[test]
    fn a_fresh_settlement_waits_for_an_earlier_open_cohort() {
        // DECISIONS K9: cohorts of bells 10 (open: 1 of 2 settled) and 12.
        let c = |bell, filed, settled| Cohort {
            bell,
            filed,
            settled,
        };
        let cs = [c(10, 2, 1), c(12, 1, 0), Cohort::default()];
        assert!(earlier_cohort_open(&cs, 12, 13), "bell 10 still open");
        assert!(!earlier_cohort_open(&cs, 10, 13), "no earlier cohort");
        assert!(
            !earlier_cohort_open(&cs, 12, 34),
            "bell 10 expired (24 bells)"
        );
        let cs = [c(10, 2, 2), c(12, 1, 0)];
        assert!(!earlier_cohort_open(&cs, 12, 13), "bell 10 settled");
    }
}
