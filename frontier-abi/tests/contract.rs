//! Offsets transcribed from the contract text (§5.3) against the layout
//! constants, the vector freshness check, and no-panic decoding of
//! arbitrary bytes (seeded xorshift, §3.5).

use frontier_abi::layout::{beacon, clash, player, province, world};
use frontier_abi::{entry, ix, log, tags::Ix};

#[test]
fn offsets_match_the_contract_text() {
    use player::{citizen as C, holding as H, transit as T};
    use province::{entry as E, province as P, site as SM};
    use world::season as S;
    let table: &[(&str, usize, usize)] = &[
        ("season.status", S::STATUS, 64),
        ("season.authority", S::AUTHORITY, 72),
        ("season.genesis_ts", S::GENESIS_TS, 144),
        ("season.quicknet_pk_hash", S::QUICKNET_PK_HASH, 184),
        ("season.genesis_round", S::GENESIS_ROUND, 232),
        ("season.march_fee", S::MARCH_FEE, 280),
        ("season.reveal_cu_limit", S::REVEAL_CU_LIMIT, 300),
        ("season.clash_close_grace", S::CLASH_CLOSE_GRACE, 328),
        ("season.params_hash", S::PARAMS_HASH, 336),
        ("season.payout_params_hash", S::PAYOUT_PARAMS_HASH, 392),
        ("season.reveal_loaded_limit", S::REVEAL_LOADED_LIMIT, 480),
        ("season.join_gate", S::JOIN_GATE, 484),
        ("frontier.acc_wedge", world::frontier::ACC_WEDGE, 152),
        ("ringseed.payer", world::ring_seed::PAYER, 80),
        (
            "pfund.provinces_funded",
            world::province_fund::PROVINCES_FUNDED,
            44,
        ),
        ("joinshard.released", world::join_shard::RELEASED, 104),
        ("beaconlog.beneficiary", world::beacon_log::BENEFICIARY, 96),
        ("dpool.claims", world::defence_pool::CLAIMS, 48),
        ("citizen.holding", C::HOLDING, 168),
        ("citizen.ticket_bell", C::TICKET_BELL, 188),
        ("citizen.ticket_next", C::TICKET_NEXT, 207),
        ("citizen.citizen_tag", C::CITIZEN_TAG, 208),
        ("citizen.ticket_escrow", C::TICKET_ESCROW, 272),
        ("citizen.ticket_funder", C::TICKET_FUNDER, 280),
        ("holding.stores", H::STORES, 152),
        ("holding.queue", H::QUEUE, 600),
        ("holding.reserve", H::RESERVE, 720),
        ("holding.transit", H::TRANSIT, 784),
        ("holding.explore", H::EXPLORE, 1168),
        ("holding.escrow", H::ESCROW, 1192),
        ("holding.rent_payer", H::RENT_PAYER, 1200),
        ("holding.final_ts", H::FINAL_TS, 1232),
        ("holding.pool_owed", H::POOL_OWED, 1240),
        ("transit.host_id", T::HOST_ID, 8),
        ("transit.seal_root", T::SEAL_ROOT, 48),
        ("transit.tip", T::TIP, 80),
        ("transit.flags", T::FLAGS, 88),
        ("province.resolved_next", P::RESOLVED_NEXT, 72),
        ("province.terrain", P::TERRAIN, 128),
        ("province.resource", P::RESOURCE, 189),
        ("province.sites", P::SITES, 250),
        ("province.site_mirror", P::SITE_MIRROR, 296),
        ("province.entries", P::ENTRIES, 1064),
        ("province.resolve_summary", P::RESOLVE_SUMMARY, 3752),
        ("province.camp", P::CAMP, 3784),
        ("province.ticket_cohorts", P::TICKET_COHORTS, 3800),
        ("site.shield_until_bell", SM::SHIELD_UNTIL_BELL, 60),
        ("entry.pend_op", E::PEND_OP, 36),
        ("entry.op_ref", E::OP_REF, 44),
        ("slot.beneficiary", clash::arrival_slot::BENEFICIARY, 56),
        ("slot.rent_to", clash::arrival_slot::RENT_TO, 88),
        ("slot.claimed", clash::arrival_slot::CLAIMED, 144),
        ("day.rent_to", clash::arrival_day::RENT_TO, 64),
        ("inputs.arrivals", clash::clash_inputs::ARRIVALS, 96),
        ("inputs.postures", clash::clash_inputs::POSTURES, 1056),
        ("inputs.resolver", clash::clash_inputs::RESOLVER, 1176),
        ("inputs.rent_to", clash::clash_inputs::RENT_TO, 1232),
        ("arrival.fate", clash::arrival::FATE, 35),
        ("anchor.rent_to", beacon::bell_anchor::RENT_TO, 96),
        ("anchor.ev_limit", beacon::bell_anchor::EV_LIMIT, 136),
        ("cache.rent_to", beacon::seed_cache::RENT_TO, 112),
        ("archive.entries", beacon::anchor_archive::ENTRIES, 64),
        ("archive.rent_to", beacon::anchor_archive::RENT_TO, 12_160),
        ("claim.count", beacon::defence_claim::COUNT, 64),
    ];
    for (name, got, want) in table {
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn vectors_are_fresh() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_abi-vectors"))
        .arg("--check")
        .output()
        .expect("run abi-vectors --check");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

#[test]
fn decoders_never_panic_on_arbitrary_bytes() {
    let mut r = XorShift(0xFACE_B00C_0000_0001);
    for _ in 0..20_000 {
        let len = (r.next() % 400) as usize;
        let mut d = r.bytes(len);
        // steer a share of the inputs to valid tags and kinds
        if !d.is_empty() && r.next() % 2 == 0 {
            d[0] = Ix::ALL[(r.next() % Ix::ALL.len() as u64) as usize].tag();
        }
        let _ = ix::tag_of(&d);
        let _ = ix::Depart::decode(&d);
        let _ = ix::Reveal::decode(&d);
        let _ = ix::SettleTransit::decode(&d);
        let _ = ix::FileTicket::decode(&d);
        let _ = ix::ArchiveAnchors::decode(&d);
        let _ = ix::CreateSeason::decode(&d);
        let _ = entry::Entry::read(&d);
        if d.len() > 1 && r.next() % 2 == 0 {
            d[0] = log::VERSION;
            d[1] = log::SPECS[(r.next() % log::SPECS.len() as u64) as usize].kind as u8;
        }
        if let Ok(rec) = log::decode(&d) {
            let _ = log::chains_of(rec.kind, rec.key, rec.payload);
        }
        // chains_of directly on arbitrary key and payload slices, every
        // kind (integ-W1 review: CLOSE indexed a short key).
        let spec = &log::SPECS[(r.next() % log::SPECS.len() as u64) as usize];
        let kl = (r.next() % 20) as usize;
        let key = r.bytes(kl);
        let _ = log::chains_of(spec.kind, &key, &d);
        let _ = log::chains_of(spec.kind, &[], &[]);
        let mut big = vec![0u8; 4_096];
        let n = d.len().min(4_096);
        big[..n].copy_from_slice(&d[..n]);
        for i in 0..56 {
            let _ = entry::read_entry(&big, i);
        }
        let _ = frontier_abi::prologue::cohort_closed(&big, r.next() as u32, r.next() as u32);
        let _ = frontier_abi::prologue::host_in_transit(&d, r.next());
    }
}
