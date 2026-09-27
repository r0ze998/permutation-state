//! The harness proving itself (no Frontier binary needed): the SIMD-0186
//! enforcement control against the numbers the W2-B validator drill
//! measured, the probe's pre-funding regression, and the build markers.
//!
//! The control tests are named `g01_loaded_limit_control_*`, so Gate W2's
//! `./run.sh --release -- g01_loaded_ …` runs them with the per-kind tests
//! ("green … with the harness's enforcement control", §12).

use permutation_frontier_svm_tests::chain::{
    assert_loaded_exceeded, expect_lands, program_id, Build, Chain, Profile, ACCOUNT_BASE,
    DRILL_COMPUTE_BUDGET_DATA, DRILL_SYSTEM_DATA, PROGRAMDATA_META,
};
use permutation_frontier_svm_tests::{probe, sha256, Address, Keypair, Signer};

/// The drill's ProgramData: `--max-len 679,936` (the 540,704-B padded probe,
/// round_up(1.25 × .so, 4 KiB)) → 679,981 B of data [measured, 2026-09-27].
const DRILL_MAX_LEN: usize = 679_936;
/// The drill's base need: payer (64) + probe program account (64 + 36) +
/// ProgramData (64 + 679,981) + ComputeBudget program (64 + 22) [measured].
const DRILL_BASE_NEED: u64 = 680_295;

fn control_chain() -> (Chain, Address, Keypair) {
    let so = probe::load();
    let mut c = Chain::bare(Build::Release);
    let program = Address::new_from_array(sha256(&[b"control probe"]));
    c.deploy(program, &so, DRILL_MAX_LEN, None);
    let payer = c.funded(b"control payer", 10);
    (c, program, payer)
}

#[test]
fn g01_loaded_limit_control_harness_rule_matches_the_validator_drill() {
    let (mut c, program, payer) = control_chain();
    let absent = Address::new_from_array(sha256(&[b"drill absent"]));
    let prefunded = Address::new_from_array(sha256(&[b"drill prefunded"]));
    c.prefund(&prefunded, 1_000_000);
    let data = Address::new_from_array(sha256(&[b"drill data 10000"]));
    let l = c.rent(10_000);
    c.put(data, Address::default(), vec![0; 10_000], l);
    let need = |c: &Chain, extra: &[Address]| {
        let t = c.transaction(
            &Profile::NONE.with_loaded(64 << 20),
            &[probe::noop(program, extra)],
            &[&payer],
        );
        c.loaded_size(&t.message)
    };
    let base = need(&c, &[]);
    assert_eq!(
        base,
        ACCOUNT_BASE
            + (ACCOUNT_BASE + 36)
            + (ACCOUNT_BASE + (PROGRAMDATA_META + DRILL_MAX_LEN) as u64)
            + (ACCOUNT_BASE + DRILL_COMPUTE_BUDGET_DATA as u64)
    );
    assert_eq!(base, DRILL_BASE_NEED, "the validator's base need");
    assert_eq!(need(&c, &[absent]), base, "absent: +0 (drill)");
    assert_eq!(need(&c, &[prefunded]), base + 64, "pre-funded: +64 (drill)");
    assert_eq!(
        need(&c, &[data]),
        base + 10_064,
        "10,000 B: +10,064 (drill)"
    );
    assert_eq!(
        need(&c, &[fclient::addr::instructions_sysvar()]),
        base,
        "instructions sysvar: +0 (drill)"
    );
    assert_eq!(
        need(&c, &[Address::default()]),
        base + 64 + DRILL_SYSTEM_DATA as u64,
        "listed System program: +78 (drill)"
    );
}

#[test]
fn g01_loaded_limit_control_one_byte_below_fails_charged_at_the_need_lands() {
    let (mut c, program, payer) = control_chain();
    let ix = probe::noop(program, &[]);
    // At the need: lands.
    let l = expect_lands(
        c.send_with(
            &Profile::NONE.with_loaded(DRILL_BASE_NEED as u32),
            std::slice::from_ref(&ix),
            &[&payer],
        ),
        "probe at the need",
    );
    assert_eq!(l.loaded, DRILL_BASE_NEED);
    // One byte below: refused as SIMD-0186 does, the fee charged.
    let before = c.lamports(&payer.pubkey());
    let f = assert_loaded_exceeded(c.send_with(
        &Profile::NONE.with_loaded(DRILL_BASE_NEED as u32 - 1),
        std::slice::from_ref(&ix),
        &[&payer],
    ));
    assert_eq!(f.fee, 5_000);
    assert_eq!(
        before - c.lamports(&payer.pubkey()),
        5_000,
        "the payer paid the fee"
    );
    // One page below round_up(need): refused too (the §13.1 form).
    let page_below = (DRILL_BASE_NEED.div_ceil(32_768) * 32_768 - 32_768) as u32;
    assert_loaded_exceeded(c.send_with(&Profile::NONE.with_loaded(page_below), &[ix], &[&payer]));
}

#[test]
fn g01_loaded_limit_control_litesvm_alone_undercounts_the_programdata() {
    // Why the harness checks: LiteSVM 0.16 does not count the invoked
    // program's ProgramData (W1-F measured it; re-proved here), so without
    // the harness a transaction ~680 KB under its need lands.
    let (mut c, program, payer) = control_chain();
    c.enforce_loaded = false;
    let listed_only = 64 + 100 + 86; // payer, program account, ComputeBudget
    let r = c.send_with(
        &Profile::NONE.with_loaded(listed_only + 1_024),
        &[probe::noop(program, &[])],
        &[&payer],
    );
    assert!(r.is_ok(), "raw LiteSVM lands it: {r:?}");
    c.enforce_loaded = true;
    assert_loaded_exceeded(c.send_with(
        &Profile::NONE.with_loaded(listed_only + 1_024),
        &[probe::noop(program, &[])],
        &[&payer],
    ));
}

#[test]
fn g02_probe_create_account_fails_on_a_prefunded_address() {
    // §13.2: "a stand-alone probe shows CreateAccount failing on the same
    // address". The probe runs at the Frontier program's id on a chain of
    // its own, so it signs as the real Season PDA and aims at the real
    // addresses (the Season PDA itself, and the with-seed Frontier account).
    let so = probe::load();
    let id = 1u64;
    let (season, bump) = fclient::addr::season_pda(&program_id(), id);
    let frontier_seed = b"fr";
    let frontier = fclient::addr::with_seed(&season, frontier_seed, &program_id());
    let a = fclient::addr::Addresses::new(program_id(), id);
    assert_eq!(
        (a.season, a.frontier()),
        (season, frontier),
        "the Frontier program's addresses"
    );
    for prefund in [0u64, 1, 11_054_080, 110_540_800] {
        let mut c = Chain::with_program(Build::Release, &so, program_id());
        let payer = c.funded(b"probe payer", 10);
        if prefund > 0 {
            c.prefund(&season, prefund);
            c.prefund(&frontier, prefund);
        }
        let r0 = c.send_with(
            &Profile::NONE,
            &[probe::create_season_pda(
                program_id(),
                payer.pubkey(),
                id,
                bump,
                2_048,
            )],
            &[&payer],
        );
        let r1 = c.send_with(
            &Profile::NONE,
            &[probe::create_with_seed(
                program_id(),
                payer.pubkey(),
                id,
                bump,
                frontier_seed,
                512,
            )],
            &[&payer],
        );
        if prefund == 0 {
            expect_lands(r0, "CreateAccount of a fresh Season PDA");
            expect_lands(r1, "CreateAccountWithSeed of a fresh address");
            assert_eq!(c.owner(&season), Some(program_id()));
        } else {
            for (what, r) in [("CreateAccount", r0), ("CreateAccountWithSeed", r1)] {
                let f = r.expect_err(what);
                assert!(
                    f.logs.iter().any(|l| l.contains("already in use")),
                    "{what} on an address holding {prefund} lamports: {f:?}"
                );
            }
            assert!(c.is_absent(&season) && c.is_absent(&frontier));
        }
    }
}

#[test]
fn build_markers_are_where_they_belong() {
    // Every build found must carry exactly its own marker (§3.2, I-53);
    // builds not present are reported, not failed (run.sh builds them all).
    for b in Build::ALL {
        let Some(p) = permutation_frontier_svm_tests::chain::so_path(b) else {
            eprintln!("{b:?}: no build found");
            continue;
        };
        let Ok(so) = std::fs::read(&p) else {
            eprintln!("{b:?}: {} missing", p.display());
            continue;
        };
        for other in Build::ALL {
            if let Some(m) = other.marker() {
                let has = permutation_frontier_svm_tests::chain::contains(&so, m);
                assert_eq!(
                    has,
                    other == b,
                    "{b:?} build {} and marker {}",
                    p.display(),
                    String::from_utf8_lossy(m)
                );
            }
        }
    }
}

#[test]
fn keypairs_and_program_id_are_reproducible() {
    assert_eq!(program_id(), program_id());
    let k: Keypair = permutation_frontier_svm_tests::keypair(b"x");
    assert_eq!(
        k.pubkey(),
        permutation_frontier_svm_tests::keypair(b"x").pubkey()
    );
}
