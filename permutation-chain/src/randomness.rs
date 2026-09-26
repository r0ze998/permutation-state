//! Tick and season randomness from the MagicBlock VRF (WP11): the state
//! codes, the derivations the program and the verifier share, the VRF
//! program's addresses and the request instruction. Always compiled (no
//! `program` feature): the verifier uses it.
//!
//! A tick's randomness is `tick_vrf(rand_pre, E, RAND_VRF)`: `rand_pre`
//! commits to the frozen input's revealed salts (`salts_hash`) and E is the
//! oracle's output for a request seeded with it. Nobody knows E before the
//! input froze, so no choice made before it (salts, which batches to reveal)
//! can steer the tick. The season seed mixes E into the registration totals
//! the same way (`season_seed`).
//!
//! The VRF program calls back with `tag ‖ E[32] ‖ callback args`, where the
//! tag is our one-byte borsh variant tag (`CONSUME_TICK_TAG`,
//! `CONSUME_SEED_TAG`), so the callback data decodes straight into
//! `ConsumeTickRandomness { randomness, season_id, tick }` /
//! `ConsumeSeasonSeed { randomness, season_id }`. That order is the one the
//! SDK documents; the devnet spike confirms it (a different order changes
//! only this module and the two callback variants).

use solana_program::hash::hashv;
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::pubkey::Pubkey;

use permutation_rules::rng::Salt;

/// `WorldMeta::rand_state`: not frozen.
pub const RAND_NONE: u8 = 0;
/// Frozen; the randomness was requested from the VRF.
pub const RAND_PENDING: u8 = 1;
/// Drawn from the VRF.
pub const RAND_VRF: u8 = 2;
/// The oracle stayed silent for `state::VRF_GIVEUP_SECONDS`: the salts alone
/// (flagged by the verifier).
pub const RAND_FALLBACK: u8 = 3;
/// `dev-randomness` builds: drawn in the freezing transaction.
pub const RAND_DEV: u8 = 4;

/// `Season::seed_state`: no seed yet.
pub const SEED_NONE: u8 = 0;
/// Requested from the VRF (status Seeding).
pub const SEED_PENDING: u8 = 1;
/// Drawn from the VRF.
pub const SEED_VRF: u8 = 2;
/// `dev-randomness` builds: from the latest slot hash, in `StartSeason`.
pub const SEED_DEV: u8 = 4;

/// The MagicBlock VRF program.
pub const VRF_PROGRAM_ID: Pubkey =
    solana_program::pubkey!("Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz");
/// The base layer's oracle queue (`DEFAULT_QUEUE`): the season seed.
pub const VRF_QUEUE_BASE: Pubkey =
    solana_program::pubkey!("Cuj97ggrhhidhbu39TijNVqE74xvKJ69gDervRUXAxGh");
/// The ER's oracle queue (`DEFAULT_EPHEMERAL_QUEUE`): tick randomness.
pub const VRF_QUEUE_ER: Pubkey =
    solana_program::pubkey!("5hBR571xnXppuCPveTrctfTU7tJLSN94nq7kv7FRK5Tc");
/// Seed of our program identity PDA (`["identity"]` of this program), which
/// signs our requests, and of the VRF's scoped identity
/// (`["identity", program_id]` of the VRF program), which signs its
/// callbacks to us.
pub const IDENTITY_SEED: &[u8] = b"identity";
/// The VRF instruction for a scoped, high-priority request (the first byte
/// of its 8-byte discriminator).
pub const REQUEST_SCOPED_HIGH_PRIORITY: u8 = 11;
/// The system program (the VRF request names it).
pub const SYSTEM_PROGRAM_ID: Pubkey = solana_program::pubkey!("11111111111111111111111111111111");
/// Our variant tags the VRF calls back (`ConsumeTickRandomness`,
/// `ConsumeSeasonSeed`).
pub const CONSUME_TICK_TAG: u8 = 29;
pub const CONSUME_SEED_TAG: u8 = 31;

/// The VRF program's identity for callbacks to `program_id`: only the VRF
/// program can sign as it.
pub fn scoped_vrf_identity(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[IDENTITY_SEED, program_id.as_ref()], &VRF_PROGRAM_ID).0
}

/// Our program identity PDA (`["identity"]`) and its bump: it signs our
/// randomness requests.
pub fn program_identity(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[IDENTITY_SEED], program_id)
}

/// Commitment to a frozen tick's revealed salts, in (civ, role) order.
pub fn salts_hash(season_id: u64, tick: u16, salts: &[Salt]) -> [u8; 32] {
    let mut parts: Vec<[u8; 35]> = Vec::with_capacity(salts.len());
    for (civ, role, salt) in salts {
        let mut p = [0u8; 35];
        p[..2].copy_from_slice(&civ.to_le_bytes());
        p[2] = *role;
        p[3..].copy_from_slice(salt);
        parts.push(p);
    }
    let (id, t) = (season_id.to_le_bytes(), tick.to_le_bytes());
    let mut all: Vec<&[u8]> = Vec::with_capacity(3 + parts.len());
    all.extend_from_slice(&[b"PS/tick-salts/v1", &id, &t]);
    all.extend(parts.iter().map(|p| &p[..]));
    hashv(&all).to_bytes()
}

/// The tick's randomness from `rand_pre` and the oracle output, by source;
/// `None` while it is not drawn (`RAND_NONE`, `RAND_PENDING`).
pub fn tick_vrf(rand_pre: &[u8; 32], oracle: &[u8; 32], state: u8) -> Option<[u8; 32]> {
    let parts: &[&[u8]] = match state {
        RAND_VRF => &[b"PS/tick-vrf/v2", rand_pre, oracle],
        RAND_FALLBACK => &[b"PS/tick-vrf/fallback", rand_pre],
        RAND_DEV => &[b"PS/tick-vrf/dev", rand_pre],
        _ => return None,
    };
    Some(hashv(parts).to_bytes())
}

/// The caller seed of the season seed's request: the registration totals,
/// fixed before the request (Register and UpdateMember need `Registering`).
pub fn season_seed_request(
    season_id: u64,
    count: u32,
    treasury_borsh: &[u8],
    prev_root: &[u8; 32],
) -> [u8; 32] {
    hashv(&[
        b"PS/season-seed-request/v7",
        &season_id.to_le_bytes(),
        &count.to_le_bytes(),
        treasury_borsh,
        prev_root,
    ])
    .to_bytes()
}

/// The season seed from the oracle's output and the registration totals.
pub fn season_seed(
    oracle: &[u8; 32],
    season_id: u64,
    count: u32,
    treasury_borsh: &[u8],
    prev_root: &[u8; 32],
) -> [u8; 32] {
    hashv(&[
        b"PS/season-seed/v7",
        oracle,
        &season_id.to_le_bytes(),
        &count.to_le_bytes(),
        treasury_borsh,
        prev_root,
    ])
    .to_bytes()
}

/// The callback args of a tick request (`season_id ‖ tick`).
pub fn tick_callback_args(season_id: u64, tick: u16) -> [u8; 10] {
    let mut a = [0u8; 10];
    a[..8].copy_from_slice(&season_id.to_le_bytes());
    a[8..].copy_from_slice(&tick.to_le_bytes());
    a
}

/// A scoped, high-priority randomness request to the VRF program, as the
/// SDK's `create_request_high_priority_scoped_randomness_ix` builds it: our
/// identity PDA signs, the oracle calls `program_id` back with variant
/// `tag`, `callback_account` (writable) and `args`.
/// Accounts: payer (s,w) · identity PDA (s) · queue (w) · system · SlotHashes.
pub fn request_ix(
    program_id: &Pubkey,
    payer: &Pubkey,
    queue: &Pubkey,
    caller_seed: [u8; 32],
    tag: u8,
    callback_account: &Pubkey,
    args: &[u8],
) -> Instruction {
    let mut data = vec![REQUEST_SCOPED_HIGH_PRIORITY, 0, 0, 0, 0, 0, 0, 0];
    data.extend_from_slice(&caller_seed);
    data.extend_from_slice(program_id.as_ref());
    // callback_discriminator: Vec<u8> = [tag]
    data.extend_from_slice(&1u32.to_le_bytes());
    data.push(tag);
    // callback_accounts_metas: one, not a signer, writable
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(callback_account.as_ref());
    data.extend_from_slice(&[0, 1]);
    // callback_args: Vec<u8>
    data.extend_from_slice(&(args.len() as u32).to_le_bytes());
    data.extend_from_slice(args);
    Instruction {
        program_id: VRF_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(program_identity(program_id).0, true),
            AccountMeta::new(*queue, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(solana_program::sysvar::slot_hashes::ID, false),
        ],
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRE: [u8; 32] = [7; 32];
    const E: [u8; 32] = [9; 32];

    #[test]
    fn salts_hash_binds_every_salt_its_place_the_tick_and_the_season() {
        let salts: Vec<Salt> = vec![(0, 0, [1; 32]), (1, 3, [2; 32])];
        let h = salts_hash(5, 17, &salts);
        assert_eq!(h, salts_hash(5, 17, &salts), "deterministic");
        assert_ne!(h, salts_hash(6, 17, &salts), "season");
        assert_ne!(h, salts_hash(5, 18, &salts), "tick");
        assert_ne!(h, salts_hash(5, 17, &[salts[1], salts[0]]), "order");
        assert_ne!(h, salts_hash(5, 17, &[(2, 0, [1; 32]), salts[1]]), "civ");
        assert_ne!(h, salts_hash(5, 17, &[(0, 1, [1; 32]), salts[1]]), "role");
        assert_ne!(h, salts_hash(5, 17, &[(0, 0, [3; 32]), salts[1]]), "salt");
        assert_ne!(h, salts_hash(5, 17, &salts[..1]), "withheld");
        assert_ne!(salts_hash(5, 17, &[]), h);
    }

    #[test]
    fn tick_vrf_depends_on_its_source() {
        assert_eq!(tick_vrf(&PRE, &E, RAND_NONE), None);
        assert_eq!(tick_vrf(&PRE, &E, RAND_PENDING), None);
        let vrf = tick_vrf(&PRE, &E, RAND_VRF).unwrap();
        let fallback = tick_vrf(&PRE, &E, RAND_FALLBACK).unwrap();
        let dev = tick_vrf(&PRE, &E, RAND_DEV).unwrap();
        assert!(vrf != fallback && vrf != dev && fallback != dev);
        // The oracle output moves the VRF value only.
        assert_ne!(tick_vrf(&PRE, &[8; 32], RAND_VRF).unwrap(), vrf);
        assert_eq!(tick_vrf(&PRE, &[8; 32], RAND_FALLBACK).unwrap(), fallback);
        assert_ne!(tick_vrf(&[6; 32], &E, RAND_VRF).unwrap(), vrf);
        assert_eq!(tick_vrf(&PRE, &E, 5), None);
    }

    #[test]
    fn season_seed_depends_on_every_input() {
        let t = borsh::to_vec(&vec![5u64, 0]).unwrap();
        let base = season_seed(&E, 1, 3, &t, &PRE);
        assert_ne!(season_seed(&[8; 32], 1, 3, &t, &PRE), base);
        assert_ne!(season_seed(&E, 2, 3, &t, &PRE), base);
        assert_ne!(season_seed(&E, 1, 4, &t, &PRE), base);
        let t2 = borsh::to_vec(&vec![5u64, 1]).unwrap();
        assert_ne!(season_seed(&E, 1, 3, &t2, &PRE), base);
        assert_ne!(season_seed(&E, 1, 3, &t, &[6; 32]), base);
        // The request's seed is a different domain.
        let req = season_seed_request(1, 3, &t, &PRE);
        assert_ne!(req, base);
        assert_ne!(season_seed_request(1, 4, &t, &PRE), req);
    }

    #[test]
    fn domains_are_separated() {
        let salts: Vec<Salt> = vec![];
        let all = [
            salts_hash(0, 0, &salts),
            tick_vrf(&PRE, &E, RAND_VRF).unwrap(),
            season_seed_request(0, 0, &[], &PRE),
            season_seed(&E, 0, 0, &[], &PRE),
        ];
        for i in 0..all.len() {
            for j in 0..i {
                assert_ne!(all[i], all[j]);
            }
        }
    }

    #[test]
    fn the_request_matches_the_vrf_layout() {
        let program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let chunk0 = Pubkey::new_unique();
        let args = tick_callback_args(42, 7);
        let ix = request_ix(
            &program,
            &payer,
            &VRF_QUEUE_ER,
            [3; 32],
            CONSUME_TICK_TAG,
            &chunk0,
            &args,
        );
        assert_eq!(ix.program_id, VRF_PROGRAM_ID);
        assert_eq!(ix.data[..8], [11, 0, 0, 0, 0, 0, 0, 0]);
        let keys: Vec<Pubkey> = ix.accounts.iter().map(|m| m.pubkey).collect();
        assert_eq!(
            keys,
            vec![
                payer,
                program_identity(&program).0,
                VRF_QUEUE_ER,
                SYSTEM_PROGRAM_ID,
                solana_program::sysvar::slot_hashes::ID
            ]
        );
        assert!(ix.accounts[0].is_signer && ix.accounts[1].is_signer);
        assert!(ix.accounts[2].is_writable && !ix.accounts[1].is_writable);
        assert_eq!(ix.data.len(), 8 + 32 + 32 + 5 + 4 + 34 + 4 + 10);
        assert_eq!(args[..8], 42u64.to_le_bytes());
        assert_eq!(args[8..], 7u16.to_le_bytes());
    }

    /// The oracle's callback tags are our instruction tags.
    #[test]
    fn callback_tags_are_the_consume_variants() {
        use crate::instruction::ChainInstruction as I;
        let tag = |ix: I| borsh::to_vec(&ix).unwrap()[0];
        assert_eq!(
            tag(I::ConsumeTickRandomness {
                randomness: E,
                season_id: 0,
                tick: 0
            }),
            CONSUME_TICK_TAG
        );
        assert_eq!(
            tag(I::ConsumeSeasonSeed {
                randomness: E,
                season_id: 0
            }),
            CONSUME_SEED_TAG
        );
    }
}

/// The constants and the request equal the MagicBlock SDK's (the SDK is a
/// `program`-feature dependency, so this runs in the default host build).
#[cfg(all(test, feature = "program"))]
mod sdk_tests {
    use super::*;
    use ephemeral_rollups_sdk::vrf::{consts, instructions};

    fn key(bytes: [u8; 32]) -> Pubkey {
        Pubkey::new_from_array(bytes)
    }

    #[test]
    fn constants_equal_the_sdk() {
        assert_eq!(key(consts::VRF_PROGRAM_ID.to_bytes()), VRF_PROGRAM_ID);
        assert_eq!(key(consts::DEFAULT_QUEUE.to_bytes()), VRF_QUEUE_BASE);
        assert_eq!(
            key(consts::DEFAULT_EPHEMERAL_QUEUE.to_bytes()),
            VRF_QUEUE_ER
        );
        assert_eq!(consts::IDENTITY, IDENTITY_SEED);
        let program = Pubkey::new_unique();
        let theirs = consts::scoped_vrf_identity(&program.to_bytes().into());
        assert_eq!(key(theirs.to_bytes()), scoped_vrf_identity(&program));
    }

    #[test]
    fn the_request_equals_the_sdk_builder() {
        let program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let chunk0 = Pubkey::new_unique();
        let args = tick_callback_args(42, 7).to_vec();
        let ours = request_ix(
            &program,
            &payer,
            &VRF_QUEUE_ER,
            [3; 32],
            CONSUME_TICK_TAG,
            &chunk0,
            &args,
        );
        let theirs = instructions::create_request_high_priority_scoped_randomness_ix(
            instructions::RequestRandomnessParams {
                payer: payer.to_bytes().into(),
                oracle_queue: VRF_QUEUE_ER.to_bytes().into(),
                callback_program_id: program.to_bytes().into(),
                callback_discriminator: vec![CONSUME_TICK_TAG],
                accounts_metas: Some(vec![
                    ephemeral_rollups_sdk::vrf::types::SerializableAccountMeta {
                        pubkey: chunk0.to_bytes().into(),
                        is_signer: false,
                        is_writable: true,
                    },
                ]),
                caller_seed: [3; 32],
                callback_args: Some(args),
            },
        );
        assert_eq!(theirs.data, ours.data);
        assert_eq!(key(theirs.program_id.to_bytes()), ours.program_id);
        let metas: Vec<(Pubkey, bool, bool)> = theirs
            .accounts
            .iter()
            .map(|m| (key(m.pubkey.to_bytes()), m.is_signer, m.is_writable))
            .collect();
        let ours: Vec<(Pubkey, bool, bool)> = ours
            .accounts
            .iter()
            .map(|m| (m.pubkey, m.is_signer, m.is_writable))
            .collect();
        assert_eq!(metas, ours);
    }
}
