//! SubmitGov through the program's dispatcher (WP03): only a member of the
//! nation with its seated key, within its quota of slots and
//! `MAX_GOV_PER_SIGNER` entries, before the deadline and never after the
//! last tick. Flips the audit's governance-flood repros (`heap_oom_repro`,
//! `gov_inbox_flood`, `gov_flood_audit`): fresh keys filled every inbox
//! and the tick input outgrew the heap.

mod play_fixture;

use permutation_chain::error::ChainError;
use permutation_chain::instruction::ChainInstruction;
use permutation_chain::processor::set_host_clock;
use permutation_chain::state::*;
use permutation_rules::gov::{GovAction, Role};
use permutation_rules::hex::Hex;
use permutation_rules::orders::Order;
use play_fixture::*;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

const NOW: i64 = 1_000;

/// Nation 0 of a 4-member world (members 0 and 2), open on `tick`, and one
/// signer slot per key: [signer, nation].
struct Gov {
    program: Pubkey,
    a: Accounts,
    keys: Vec<[u8; 32]>,
}

const SIGNER: usize = 0;
const NATION: usize = 1;

impl Gov {
    fn new(tick: u16) -> Gov {
        set_host_clock(NOW);
        let program = Pubkey::new_unique();
        let keys: Vec<[u8; 32]> = (0..4).map(|i| [0x10 + i as u8; 32]).collect();
        let state = world(&keys);
        let (key, n) = open_nation(&program, &state, 0, tick, NOW + 30);
        let mut a = Accounts::default();
        a.push(Pubkey::default(), Pubkey::default(), true, vec![]);
        a.push(key, program, false, nation_data(&n));
        Gov { program, a, keys }
    }

    fn nation(&self) -> NationAccount {
        self.a.load(NATION)
    }

    /// SubmitGov signed by `signer` for `member`.
    fn send(
        &mut self,
        signer: [u8; 32],
        member: u32,
        action: GovAction,
    ) -> Result<(), ProgramError> {
        self.a.keys[SIGNER] = Pubkey::new_from_array(signer);
        run(
            &self.program,
            &mut self.a,
            &[SIGNER, NATION],
            &ChainInstruction::SubmitGov { member, action },
        )
    }
}

fn err(e: ChainError) -> Result<(), ProgramError> {
    Err(e.into())
}

fn stand() -> GovAction {
    GovAction::Stand { roles: 1 }
}

fn propose(n: usize, hexes: usize) -> GovAction {
    GovAction::Propose {
        role: Role::General,
        orders: (0..n)
            .map(|u| Order::MoveUnit {
                unit: u as u32,
                path: vec![Hex { q: 1, r: 1 }; hexes],
            })
            .collect(),
    }
}

/// Fresh keys (the flood's) write nothing, whatever member they name.
#[test]
fn fresh_keys_are_refused() {
    let mut g = Gov::new(0);
    for k in 0..200u32 {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(&k.to_le_bytes());
        key[4] = 0xee;
        assert_eq!(g.send(key, k % 4, stand()), err(ChainError::Unauthorized));
    }
    // A member's key naming another member, and a member of the other
    // nation with its own key.
    let (k0, k1, k2) = (g.keys[0], g.keys[1], g.keys[2]);
    assert_eq!(g.send(k0, 2, stand()), err(ChainError::Unauthorized));
    assert_eq!(g.send(k1, 1, stand()), err(ChainError::Unauthorized));
    assert!(g.nation().inbox.is_empty());
    // The members themselves.
    assert_eq!(g.send(k0, 0, stand()), Ok(()));
    assert_eq!(g.send(k2, 2, stand()), Ok(()));
    assert_eq!(g.nation().inbox.len(), 2);
}

/// Every member's quota, never shared; entries past it are InboxFull.
#[test]
fn the_quota_is_per_member() {
    let mut g = Gov::new(0);
    let quota = g.nation().gov_quota;
    assert_eq!(quota, gov_quota(4, 2));
    let (k0, k2) = (g.keys[0], g.keys[2]);
    // Slots: 1 per entry and per proposed order, at least one per 48 B.
    assert_eq!(gov_slots(&stand()), Some(1));
    assert_eq!(gov_slots(&propose(1, 12)), Some(4));
    assert_eq!(gov_slots(&propose(2, 12)), Some(6));
    assert_eq!(gov_slots(&propose(4, 12)), Some(10));
    assert_eq!(
        gov_slots(&propose(4, 20)),
        None,
        "over MAX_GOV_ACTION_BYTES"
    );
    assert_eq!(g.send(k0, 0, propose(4, 12)), Ok(()));
    let left = quota - 10;
    for _ in 0..left {
        assert_eq!(g.send(k0, 0, stand()), Ok(()));
    }
    assert_eq!(g.send(k0, 0, stand()), err(ChainError::InboxFull));
    // Member 2's quota is untouched; its entry count caps at MAX_GOV_PER_SIGNER.
    for _ in 0..MAX_GOV_PER_SIGNER {
        assert_eq!(g.send(k2, 2, stand()), Ok(()));
    }
    assert_eq!(g.send(k2, 2, stand()), err(ChainError::InboxFull));
    let n = g.nation();
    assert_eq!(n.inbox.len(), 1 + left as usize + MAX_GOV_PER_SIGNER);
    // Every member at its quota still leaves each office's reveal room.
    let used = borsh::object_length(&n).unwrap();
    assert!(used + 4 * REVEAL_ROOM <= NATION_SPACE, "{used} B");
}

#[test]
fn proposals_are_bounded() {
    let mut g = Gov::new(0);
    let k0 = g.keys[0];
    let most = rules().max_proposal_orders as usize;
    assert_eq!(g.send(k0, 0, propose(0, 1)), err(ChainError::InvalidParams));
    assert_eq!(
        g.send(k0, 0, propose(most + 1, 1)),
        err(ChainError::InvalidParams)
    );
    assert_eq!(
        g.send(k0, 0, propose(4, 20)),
        err(ChainError::InvalidParams)
    );
    assert_eq!(g.send(k0, 0, propose(most, 12)), Ok(()));
}

/// Governance closes at the deadline and while the commitments are closed
/// or the input frozen; after the last tick nothing is queued at all.
#[test]
fn governance_closes() {
    let mut g = Gov::new(0);
    let k0 = g.keys[0];
    set_host_clock(NOW + 30);
    assert_eq!(g.send(k0, 0, stand()), err(ChainError::TickFrozen));
    set_host_clock(NOW);
    for flag in [0, 1] {
        let mut n = g.nation();
        (n.revealing, n.frozen) = (flag == 0, flag == 1);
        g.a.store(NATION, &n);
        assert_eq!(g.send(k0, 0, stand()), err(ChainError::TickFrozen));
    }
    let mut last = Gov::new(rules().ticks_per_season);
    let k = last.keys[0];
    assert_eq!(last.send(k, 0, stand()), err(ChainError::WrongStatus));
    let outsider = [0xee; 32];
    assert_eq!(
        last.send(outsider, 0, stand()),
        err(ChainError::WrongStatus)
    );
    let mut n = last.nation();
    n.open_tick = NO_TICK;
    last.a.store(NATION, &n);
    assert_eq!(last.send(k, 0, stand()), err(ChainError::WrongStatus));
}
