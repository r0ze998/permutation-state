//! A native model of the program's beacon-side instructions, registered as
//! a LiteSVM builtin on `localnet` while the program (W2-A) is built in
//! parallel. It follows the contract's account lists, checks, layouts
//! (`frontier-abi` offsets) and address grammar, so the keeper talks to it
//! exactly as to the program; it is **not** the program:
//!
//! | tag | modelled | left out |
//! |---|---|---|
//! | 0x08 AnnounceSeason | PDA create, lead ≥ 24 h, bond ≥ 1 SOL | the upgrade-authority check (a builtin has no ProgramData) |
//! | 0x01 CreateSeason | params hash, `SeasonParams::validate`, Frontier, 6 funds, pool, genesis round and ts | PayoutParams validation, logs |
//! | 0x09 InitBeaconLogs | 16 logs | — |
//! | 0x03 ConsumeGenesisSeed | round rule, BLS verify with the test key, seed | logs, evidence |
//! | 0x10 / 0x11 / 0x12 / 0x13 | canonical addresses, present → success no-op, tombstone, round rules, verify, rent from the fee payer (`rent_to`), `ANCHOR`/`SEED`/`BEACON` records | the instructions-sysvar evidence (stored 0) |
//! | 0x02, 0x20–0x23, 0x30, 0x33–0x35, 0x43, 0x46–0x48, 0x55 | land (W3-C): see `land.rs` | as listed there |
//! | 0x14 ArchiveAnchors | archive init (`rent_to` = payer), `now ≥ A + archive_after`, the cache of THE anchor, entry `{a_off, seed, sig}`, tombstone and archived bits first, anchor closed to its `rent_to` | the `ARCHIVE`/`CLOSE` records |
//! | 0x15 CloseSeedCache | archived bit, `rent_to` match, close | the `CLOSE` record |
//! | 0x44, 0x50–0x54, 0x60–0x66, 0x70 | play (W4-C): see `play.rs` | as listed there |
//! | other | `NotImplemented` (99) | — |
//!
//! Compute: each instruction consumes roughly the program's measured cost
//! (≈ 330k for a verifying one, SP-V2) so block and per-account caps apply.

#![allow(dead_code)]

mod land;
mod play;

use std::sync::OnceLock;

use solana_address::Address;
use solana_instruction::error::InstructionError;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_runtime::declare_process_instruction;
use solana_program_runtime::invoke_context::InvokeContext;

use fclient::beacon::{self, TestKey};
use fclient::clock::Drand;
use frontier_abi::layout::beacon::{
    anchor_archive as arc, archive_entry as ent, bell_anchor as ban, seed_cache as sdc,
};
use frontier_abi::layout::world::{
    beacon_log as blg, defence_pool as dpl, frontier as frt, province_fund as pfd, season as ssn,
};

type R<T> = Result<T, InstructionError>;

const BAD_DATA: u32 = 1;
const BAD_ACCOUNT: u32 = 2;
const BAD_ADDRESS: u32 = 3;
const AUTH: u32 = 4;
const WRONG_STATUS: u32 = 5;
const WRONG_ROUND: u32 = 7;
const NO_ANCHOR: u32 = 8;
const CRYPTO: u32 = 9;
const ARCHIVED: u32 = 16;
const ANNOUNCE: u32 = 56;
const NOT_IMPLEMENTED: u32 = 99;

fn e(c: u32) -> InstructionError {
    InstructionError::Custom(c)
}

fn test_pk() -> &'static [u8; 96] {
    static PK: OnceLock<[u8; 96]> = OnceLock::new();
    PK.get_or_init(|| TestKey::new().pk96)
}

pub fn rent(space: usize) -> u64 {
    (128 + space as u64) * 5_080
}

declare_process_instruction!(ModelProgram, 0, |ic| { process(ic) });

// ------------------------------------------------------------------ access

fn key(ic: &InvokeContext, i: u16) -> R<Address> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    Ok(*c.get_key_of_instruction_account(i)?)
}

fn program_id(ic: &InvokeContext) -> R<Address> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    Ok(*c.get_program_key()?)
}

fn n_accounts(ic: &InvokeContext) -> R<u16> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    Ok(c.get_number_of_instruction_accounts())
}

fn ix_data(ic: &InvokeContext) -> R<Vec<u8>> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    Ok(c.get_instruction_data().to_vec())
}

fn is_signer(ic: &InvokeContext, i: u16) -> R<bool> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    c.is_instruction_account_signer(i)
}

/// `(owner, data, lamports)`.
fn read(ic: &InvokeContext, i: u16) -> R<(Address, Vec<u8>, u64)> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    let a = c.try_borrow_instruction_account(i)?;
    Ok((*a.get_owner(), a.get_data().to_vec(), a.get_lamports()))
}

fn write(ic: &InvokeContext, i: u16, data: &[u8]) -> R<()> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    let mut a = c.try_borrow_instruction_account(i)?;
    a.set_data_from_slice(data)
}

fn move_lamports(ic: &InvokeContext, from: u16, to: u16, n: u64) -> R<()> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    {
        let mut a = c.try_borrow_instruction_account(from)?;
        a.checked_sub_lamports(n)?;
    }
    let mut b = c.try_borrow_instruction_account(to)?;
    b.checked_add_lamports(n)
}

fn clock(ic: &InvokeContext) -> R<(u64, i64)> {
    let c = ic.environment_config.sysvar_cache().get_clock()?;
    Ok((c.slot, c.unix_timestamp))
}

fn log_ps2(ic: &InvokeContext, body: &[u8]) {
    solana_program_runtime::stable_log::program_data(&ic.get_log_collector(), &[b"PS2", body]);
}

fn consume(ic: &InvokeContext, n: u64) -> R<()> {
    ic.compute_meter
        .consume_checked(n)
        .map_err(|_| InstructionError::ComputationalBudgetExceeded)
}

fn is_absent(owner: &Address, data: &[u8]) -> bool {
    *owner == fclient::addr::system_program() && data.is_empty()
}

// ------------------------------------------------------------------ system CPI

fn system_ix(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: fclient::addr::system_program(),
        accounts,
        data,
    }
}

fn transfer_ix(from: Address, to: Address, lamports: u64) -> Instruction {
    let mut d = 2u32.to_le_bytes().to_vec();
    d.extend_from_slice(&lamports.to_le_bytes());
    system_ix(
        vec![AccountMeta::new(from, true), AccountMeta::new(to, false)],
        d,
    )
}

fn allocate_with_seed_ix(
    target: Address,
    base: Address,
    seed: &[u8],
    space: u64,
    owner: Address,
) -> Instruction {
    let mut d = 9u32.to_le_bytes().to_vec();
    d.extend_from_slice(base.as_ref());
    d.extend_from_slice(&(seed.len() as u64).to_le_bytes());
    d.extend_from_slice(seed);
    d.extend_from_slice(&space.to_le_bytes());
    d.extend_from_slice(owner.as_ref());
    system_ix(
        vec![
            AccountMeta::new(target, false),
            AccountMeta::new_readonly(base, true),
        ],
        d,
    )
}

fn allocate_ix(target: Address, space: u64) -> Instruction {
    let mut d = 8u32.to_le_bytes().to_vec();
    d.extend_from_slice(&space.to_le_bytes());
    system_ix(vec![AccountMeta::new(target, true)], d)
}

fn assign_ix(target: Address, owner: Address) -> Instruction {
    let mut d = 1u32.to_le_bytes().to_vec();
    d.extend_from_slice(owner.as_ref());
    system_ix(vec![AccountMeta::new(target, true)], d)
}

struct SeasonRef {
    id: u64,
    bump: u8,
    key: Address,
}

impl SeasonRef {
    fn seeds(&self) -> ([u8; 8], [u8; 1]) {
        (self.id.to_le_bytes(), [self.bump])
    }
}

/// `init_with_seed` (§4.2): the payer tops the target up to `rent(space) +
/// extra` (pre-funded lamports count), then the Season PDA allocates it.
fn init_with_seed(
    ic: &mut InvokeContext,
    payer: u16,
    target: u16,
    season: &SeasonRef,
    seed: &[u8],
    space: usize,
    extra: u64,
) -> R<()> {
    let (tk, pk, prog) = (key(ic, target)?, key(ic, payer)?, program_id(ic)?);
    let (owner, data, lamports) = read(ic, target)?;
    if !is_absent(&owner, &data) {
        return Err(e(BAD_ACCOUNT));
    }
    if fclient::addr::with_seed(&season.key, seed, &prog) != tk {
        return Err(e(BAD_ADDRESS));
    }
    let need = (rent(space) + extra).saturating_sub(lamports);
    if need > 0 {
        ic.native_invoke_signed(transfer_ix(pk, tk, need), &[])?;
    }
    let (id, bump) = season.seeds();
    ic.native_invoke_signed(
        allocate_with_seed_ix(tk, season.key, seed, space as u64, prog),
        &[&[ssn::PDA_PREFIX, &id, &bump]],
    )
}

// ------------------------------------------------------------------ bytes

fn put<const N: usize>(d: &mut [u8], o: usize, v: [u8; N]) {
    d[o..o + N].copy_from_slice(&v);
}
fn get<const N: usize>(d: &[u8], o: usize) -> [u8; N] {
    d[o..o + N].try_into().expect("in bounds")
}
fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(get(d, o))
}
fn u64_at(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(get(d, o))
}
fn i64_at(d: &[u8], o: usize) -> i64 {
    i64::from_le_bytes(get(d, o))
}

fn header(d: &mut [u8], magic: [u8; 8], season: u64, chained: bool) {
    put(d, 0, magic);
    put(d, 8, season.to_le_bytes());
    if chained {
        put(d, 16, 1u16.to_le_bytes());
    }
}

struct Cursor<'a>(&'a [u8], usize);
impl Cursor<'_> {
    fn take(&mut self, n: usize) -> R<&[u8]> {
        let s = self.0.get(self.1..self.1 + n).ok_or(e(BAD_DATA))?;
        self.1 += n;
        Ok(s)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }
    fn i16(&mut self) -> R<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }
    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }
    fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn i64(&mut self) -> R<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn arr<const N: usize>(&mut self) -> R<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("N"))
    }
    fn done(&self) -> R<()> {
        (self.1 == self.0.len()).then_some(()).ok_or(e(BAD_DATA))
    }
}

struct Beacon {
    round: u64,
    sig48: [u8; 48],
}

fn beacon_arg(c: &mut Cursor) -> R<Beacon> {
    let round = c.u64()?;
    let sig48 = c.arr::<48>()?;
    c.take(fclient::ix::HINTS_LEN)?;
    Ok(Beacon { round, sig48 })
}

fn verify(b: &Beacon) -> R<[u8; 32]> {
    if !beacon::verify(b.round, &b.sig48, test_pk()) {
        return Err(e(CRYPTO));
    }
    let sig96 = beacon::decompress_sig(&b.sig48).ok_or(e(CRYPTO))?;
    Ok(beacon::seed_of(b.round, &sig96))
}

// ------------------------------------------------------------------ season

/// The Season at account `i`, checked canonical for this program.
fn season_at(ic: &InvokeContext, i: u16) -> R<(SeasonRef, Vec<u8>)> {
    let (owner, d, _) = read(ic, i)?;
    let prog = program_id(ic)?;
    if owner != prog || d.len() != ssn::SIZE || get::<8>(&d, 0) != ssn::MAGIC {
        return Err(e(BAD_ACCOUNT));
    }
    let id = u64_at(&d, ssn::SEASON_ID);
    let (k, bump) = fclient::addr::season_pda(&prog, id);
    if k != key(ic, i)? || bump != d[ssn::BUMP] {
        return Err(e(BAD_ADDRESS));
    }
    Ok((SeasonRef { id, bump, key: k }, d))
}

fn season_clock(d: &[u8]) -> fclient::clock::SeasonClock {
    fclient::clock::SeasonClock {
        drand: Drand {
            genesis: i64_at(d, ssn::DRAND_GENESIS),
            period: u32_at(d, ssn::DRAND_PERIOD),
        },
        genesis_ts: i64_at(d, ssn::GENESIS_TS),
        reveal_window: u32_at(d, ssn::REVEAL_WINDOW),
        window_next: u32_at(d, ssn::WINDOW_NEXT),
        window_from_bell: u32_at(d, ssn::WINDOW_FROM_BELL),
        seed_margin: u32_at(d, ssn::SEED_MARGIN),
        archive_after: u32_at(d, ssn::ARCHIVE_AFTER),
    }
}

fn effective(d: &[u8], now: i64) -> u8 {
    ssn::effective_status(d[ssn::STATUS], i64_at(d, ssn::GENESIS_TS), now)
}

fn seed_str(kind: fclient::addr::SeedKind, raw: &[u8]) -> Vec<u8> {
    let (s, n) = fclient::addr::seed(kind, raw);
    s[..n].to_vec()
}

// ------------------------------------------------------------------ dispatch

fn process(ic: &mut InvokeContext) -> R<()> {
    let data = ix_data(ic)?;
    let tag = *data.first().ok_or(e(BAD_DATA))?;
    let mut c = Cursor(&data, 1);
    match tag {
        0x08 => announce(ic, &mut c),
        0x01 => create(ic, &data[1..]),
        0x09 => init_logs(ic),
        0x03 => genesis_seed(ic, &mut c),
        0x10 => post_anchor(ic, &mut c, false),
        0x11 => post_anchor(ic, &mut c, true),
        0x12 => post_seed(ic, &mut c),
        0x13 => post_beacon(ic, &mut c),
        0x02 => land::init_shards(ic, &mut c),
        0x20 => land::open_ring(ic, &mut c),
        0x21 => land::consume_ring_seed(ic, &mut c),
        0x22 => land::open_province(ic, &mut c),
        0x23 => land::fold(ic, &mut c),
        0x30 => land::join(ic, &mut c),
        0x33 => land::file_ticket(ic, &mut c),
        0x34 => land::settle_ticket(ic, &mut c),
        0x35 => land::release_dormant(ic, &mut c),
        0x43 => land::muster(ic, &mut c),
        0x46 => land::explore(ic, &mut c),
        0x47 => land::settle_explore(ic, &mut c),
        0x48 => land::disband_stranded(ic, &mut c),
        0x55 => land::sweep_pool_owed(ic, &mut c),
        0x44 => play::dissolve(ic, &mut c),
        0x50 => play::depart(ic, &mut c),
        0x51 => play::reveal(ic, &mut c),
        0x52 => play::settle_departure(ic, &mut c),
        0x54 => play::settle_transit(ic, &mut c),
        0x60 => play::gather(ic, &mut c),
        0x61 => play::resolve(ic, &mut c),
        0x63 => play::skip(ic, &mut c),
        0x64 => play::close_clash_inputs(ic, &mut c),
        0x65 => play::close_arrival_day(ic, &mut c),
        0x66 => play::close_arrival_slot(ic, &mut c),
        0x70 => play::claim_defence(ic, &mut c),
        0x14 => archive_anchors(ic, &mut c),
        0x15 => close_seed_cache(ic, &mut c),
        _ => Err(e(NOT_IMPLEMENTED)),
    }
}

fn announce(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 15_000)?;
    let id = c.u64()?;
    let params_hash = c.arr::<32>()?;
    let t_create_min = c.i64()?;
    let bond = c.u64()?;
    c.done()?;
    if !is_signer(ic, 0)? {
        return Err(e(AUTH));
    }
    let (prog, auth, sk) = (program_id(ic)?, key(ic, 0)?, key(ic, 1)?);
    let (want, bump) = fclient::addr::season_pda(&prog, id);
    if sk != want {
        return Err(e(BAD_ADDRESS));
    }
    let (owner, d, lamports) = read(ic, 1)?;
    if !is_absent(&owner, &d) {
        return Err(e(ANNOUNCE));
    }
    let (_, now) = clock(ic)?;
    if t_create_min < now + 86_400 || bond < 1_000_000_000 {
        return Err(e(ANNOUNCE));
    }
    let need = (rent(ssn::SIZE) + bond).saturating_sub(lamports);
    ic.native_invoke_signed(transfer_ix(auth, sk, need), &[])?;
    let (idb, bb) = (id.to_le_bytes(), [bump]);
    let seeds: &[&[u8]] = &[ssn::PDA_PREFIX, &idb, &bb];
    ic.native_invoke_signed(allocate_ix(sk, ssn::SIZE as u64), &[seeds])?;
    ic.native_invoke_signed(assign_ix(sk, prog), &[seeds])?;
    let mut s = vec![0u8; ssn::SIZE];
    header(&mut s, ssn::MAGIC, id, true);
    s[ssn::STATUS] = ssn::STATUS_ANNOUNCED;
    s[ssn::BUMP] = bump;
    put(&mut s, ssn::AUTHORITY, auth.to_bytes());
    put(&mut s, ssn::PARAMS_HASH, params_hash);
    put(&mut s, ssn::T_CREATE_MIN, t_create_min.to_le_bytes());
    put(&mut s, ssn::ANNOUNCED_TS, now.to_le_bytes());
    put(&mut s, ssn::CREATION_BOND, bond.to_le_bytes());
    write(ic, 1, &s)
}

fn create(ic: &mut InvokeContext, params: &[u8]) -> R<()> {
    consume(ic, 40_000)?;
    use frontier_abi::presets::{params_hash, SeasonParams, SEASON_PARAMS_LEN};
    if n_accounts(ic)? != 11 || params.len() < SEASON_PARAMS_LEN {
        return Err(e(BAD_DATA));
    }
    let (season, mut s) = season_at(ic, 1)?;
    if s[ssn::STATUS] != ssn::STATUS_ANNOUNCED {
        return Err(e(WRONG_STATUS));
    }
    if !is_signer(ic, 0)? || key(ic, 0)?.to_bytes() != get::<32>(&s, ssn::AUTHORITY) {
        return Err(e(AUTH));
    }
    let (_, now) = clock(ic)?;
    let tcm = i64_at(&s, ssn::T_CREATE_MIN);
    if now < tcm || now >= tcm + 7 * 86_400 {
        return Err(e(ANNOUNCE));
    }
    let sp: [u8; SEASON_PARAMS_LEN] = params[..SEASON_PARAMS_LEN].try_into().expect("len");
    if params_hash(&sp, &params[SEASON_PARAMS_LEN..]) != get::<32>(&s, ssn::PARAMS_HASH) {
        return Err(e(ANNOUNCE));
    }
    let p = SeasonParams::from_bytes(&sp).ok_or(e(BAD_DATA))?;
    p.validate().map_err(|_| e(BAD_DATA))?;
    if p.quicknet_pk_hash != beacon::pk_hash(test_pk()) {
        return Err(e(BAD_DATA)); // the test-beacon build pins the test key
    }
    use fclient::addr::SeedKind;
    init_with_seed(
        ic,
        0,
        2,
        &season,
        &seed_str(SeedKind::Frontier, &[]),
        frt::SIZE,
        0,
    )?;
    let fund = p.pfund_initial / 6;
    for w in 0..6u8 {
        init_with_seed(
            ic,
            0,
            3 + w as u16,
            &season,
            &seed_str(SeedKind::ProvinceFund, &[w]),
            pfd::SIZE,
            fund,
        )?;
        let mut d = vec![0u8; pfd::SIZE];
        header(&mut d, pfd::MAGIC, season.id, false);
        d[pfd::WEDGE] = w;
        put(&mut d, pfd::FUNDED_TOTAL, fund.to_le_bytes());
        write(ic, 3 + w as u16, &d)?;
    }
    init_with_seed(
        ic,
        0,
        9,
        &season,
        &seed_str(SeedKind::DefencePool, &[]),
        dpl::SIZE,
        p.dpool_initial,
    )?;
    let mut d = vec![0u8; dpl::SIZE];
    header(&mut d, dpl::MAGIC, season.id, false);
    put(
        &mut d,
        dpl::PER_BELL_REGION_CAP,
        p.per_bell_region_cap.to_le_bytes(),
    );
    put(
        &mut d,
        dpl::PER_KEEPER_DAY_CAP,
        p.per_keeper_day_cap.to_le_bytes(),
    );
    write(ic, 9, &d)?;
    let mut f = vec![0u8; frt::SIZE];
    header(&mut f, frt::MAGIC, season.id, true);
    write(ic, 2, &f)?;
    // The Season.
    let dr = Drand {
        genesis: p.drand_genesis,
        period: p.drand_period,
    };
    let g = dr.genesis_seed_round(tcm, p.seed_margin);
    let genesis_ts = dr.round_time(g) + 600;
    s[ssn::STATUS] = ssn::STATUS_CREATED;
    s[ssn::REGIONS] = p.regions;
    s[ssn::GENESIS_RING] = p.genesis_ring;
    put(&mut s, ssn::R_MAX, p.r_max.to_le_bytes());
    s[ssn::OFFICE_TERMS_PER_WALLET] = p.office_terms_per_wallet;
    put(
        &mut s,
        ssn::RULESET_HASH,
        frontier_abi::presets::RULESET_HASH,
    );
    put(
        &mut s,
        ssn::RULES_VERSION,
        frontier_abi::presets::RULES_VERSION.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::PROGRAM_VERSION,
        p.program_version.to_le_bytes(),
    );
    put(&mut s, ssn::BELL_SECS, p.bell_secs.to_le_bytes());
    put(&mut s, ssn::GENESIS_TS, genesis_ts.to_le_bytes());
    put(&mut s, ssn::CREATED_TS, now.to_le_bytes());
    put(
        &mut s,
        ssn::JOIN_CLOSE_BELL,
        p.join_close_bell.to_le_bytes(),
    );
    put(&mut s, ssn::END_BELL, p.end_bell.to_le_bytes());
    put(&mut s, ssn::DRAND_GENESIS, p.drand_genesis.to_le_bytes());
    put(&mut s, ssn::DRAND_PERIOD, p.drand_period.to_le_bytes());
    s[ssn::NETWORK] = p.network;
    put(&mut s, ssn::QUICKNET_PK_HASH, p.quicknet_pk_hash);
    put(&mut s, ssn::REVEAL_WINDOW, p.reveal_window.to_le_bytes());
    put(&mut s, ssn::SEED_MARGIN, p.seed_margin.to_le_bytes());
    put(&mut s, ssn::WINDOW_NEXT, p.reveal_window.to_le_bytes());
    put(
        &mut s,
        ssn::WINDOW_FROM_BELL,
        ssn::WINDOW_NONE.to_le_bytes(),
    );
    put(&mut s, ssn::GENESIS_ROUND, g.to_le_bytes());
    put(&mut s, ssn::ARCHIVE_AFTER, p.archive_after.to_le_bytes());
    s[ssn::MIN_LEAD] = p.min_lead;
    s[ssn::MAX_LEAD] = p.max_lead;
    s[ssn::TRANSIT_SLOTS] = p.transit_slots;
    put(&mut s, ssn::MARCH_FEE, p.march_fee.to_le_bytes());
    put(&mut s, ssn::SEAL_BOND, p.seal_bond.to_le_bytes());
    put(
        &mut s,
        ssn::MIN_REVEAL_PRIORITY_MILLI,
        p.min_reveal_priority_milli.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::REVEAL_CU_LIMIT,
        p.reveal_cu_limit.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::BUCKET_RATE_PER_H,
        p.bucket_rate_per_h.to_le_bytes(),
    );
    put(&mut s, ssn::BUCKET_BURST, p.bucket_burst.to_le_bytes());
    put(
        &mut s,
        ssn::DEFENCE_CAP_MILLI,
        p.defence_cap_milli.to_le_bytes(),
    );
    s[ssn::LATENESS_SLOTS] = p.lateness_slots;
    put(
        &mut s,
        ssn::THETA_EARLY_BPS,
        p.theta_early_bps.to_le_bytes(),
    );
    put(&mut s, ssn::THETA_LATE_BPS, p.theta_late_bps.to_le_bytes());
    put(
        &mut s,
        ssn::THETA_SWITCH_SECS,
        p.theta_switch_secs.to_le_bytes(),
    );
    put(&mut s, ssn::RESERVE_BPS, p.reserve_bps.to_le_bytes());
    put(&mut s, ssn::EXTRA_FREE_BPS, p.extra_free_bps.to_le_bytes());
    put(
        &mut s,
        ssn::CLASH_CLOSE_GRACE,
        p.clash_close_grace.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::CAMP_REGROW_BELLS,
        p.camp_regrow_bells.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::DORMANT_AFTER_SECS,
        p.dormant_after_secs.to_le_bytes(),
    );
    put(
        &mut s,
        ssn::RELEASE_AFTER_SECS,
        p.release_after_secs.to_le_bytes(),
    );
    put(&mut s, ssn::PFUND_INITIAL, p.pfund_initial.to_le_bytes());
    put(&mut s, ssn::DPOOL_INITIAL, p.dpool_initial.to_le_bytes());
    put(
        &mut s,
        ssn::REVEAL_LOADED_LIMIT,
        p.reveal_loaded_limit.to_le_bytes(),
    );
    put(&mut s, ssn::JOIN_GATE, p.join_gate);
    write(ic, 1, &s)
}

fn init_logs(ic: &mut InvokeContext) -> R<()> {
    consume(ic, 40_000)?;
    let (season, s) = season_at(ic, 1)?;
    if !is_signer(ic, 0)? || key(ic, 0)?.to_bytes() != get::<32>(&s, ssn::AUTHORITY) {
        return Err(e(AUTH));
    }
    for r in 0..16u8 {
        init_with_seed(
            ic,
            0,
            2 + r as u16,
            &season,
            &seed_str(fclient::addr::SeedKind::BeaconLog, &[r]),
            blg::SIZE,
            0,
        )?;
        let mut d = vec![0u8; blg::SIZE];
        header(&mut d, blg::MAGIC, season.id, false);
        d[blg::REGION] = r;
        write(ic, 2 + r as u16, &d)?;
    }
    Ok(())
}

fn genesis_seed(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 330_000)?;
    let b = beacon_arg(c)?;
    c.done()?;
    let (_, mut s) = season_at(ic, 1)?;
    if s[ssn::STATUS] != ssn::STATUS_CREATED {
        return Err(e(WRONG_STATUS));
    }
    if b.round != u64_at(&s, ssn::GENESIS_ROUND) {
        return Err(e(WRONG_ROUND));
    }
    let seed = verify(&b)?;
    put(&mut s, ssn::GENESIS_SEED, seed);
    s[ssn::STATUS] = ssn::STATUS_SEEDED;
    write(ic, 1, &s)
}

fn anchor_body(bell: u32, region: u8, round: u64, a: i64, slot: u64, ben: [u8; 32]) -> Vec<u8> {
    let mut b = vec![1u8, 50];
    b.extend_from_slice(&bell.to_le_bytes());
    b.extend_from_slice(&bell.to_le_bytes());
    b.push(region);
    b.extend_from_slice(&round.to_le_bytes());
    b.extend_from_slice(&a.to_le_bytes());
    b.extend_from_slice(&slot.to_le_bytes());
    b.extend_from_slice(&ben);
    b.push(0);
    b
}

/// PostAnchor (single) or PostAnchorMulti.
fn post_anchor(ic: &mut InvokeContext, c: &mut Cursor, multi: bool) -> R<()> {
    consume(ic, 330_000)?;
    let (regions, bell, b, ben) = if multi {
        let bell = c.u32()?;
        let b = beacon_arg(c)?;
        let mask = c.u16()?;
        let ben = c.arr::<32>()?;
        c.done()?;
        let rs: Vec<u8> = (0..16u8).filter(|r| mask & (1 << r) != 0).collect();
        if rs.is_empty() || rs.len() > frontier_abi::budgets::MULTI_MAX_REGIONS {
            return Err(e(BAD_DATA));
        }
        (rs, bell, b, ben)
    } else {
        let region = c.u8()?;
        let bell = c.u32()?;
        let b = beacon_arg(c)?;
        let ben = c.arr::<32>()?;
        c.done()?;
        (vec![region], bell, b, ben)
    };
    let k = regions.len() as u16;
    if n_accounts(ic)? != 2 + 2 * k + 2 || !is_signer(ic, 0)? {
        return Err(e(BAD_ACCOUNT));
    }
    let (season, s) = season_at(ic, 1)?;
    let (slot, now) = clock(ic)?;
    if !(ssn::STATUS_SEEDED..=ssn::STATUS_RUNNING).contains(&effective(&s, now)) {
        return Err(e(WRONG_STATUS));
    }
    let sc = season_clock(&s);
    if b.round != sc.tlock_round(bell) {
        return Err(e(WRONG_ROUND));
    }
    let prog = program_id(ic)?;
    let payer = key(ic, 0)?;
    let mut verified = false;
    for (j, &r) in regions.iter().enumerate() {
        let (ai, xi) = (2 + j as u16, 2 + k + j as u16);
        let seed = seed_str(
            fclient::addr::SeedKind::BellAnchor,
            &fclient::addr::raw_anchor(bell, r),
        );
        if fclient::addr::with_seed(&season.key, &seed, &prog) != key(ic, ai)? {
            return Err(e(BAD_ADDRESS));
        }
        let arch_seed = seed_str(
            fclient::addr::SeedKind::AnchorArchive,
            &fclient::addr::raw_archive(r, arc::part_of(bell)),
        );
        if fclient::addr::with_seed(&season.key, &arch_seed, &prog) != key(ic, xi)? {
            return Err(e(BAD_ADDRESS));
        }
        let (ao, ad, _) = read(ic, ai)?;
        if !is_absent(&ao, &ad) {
            continue; // present: success, no-op
        }
        let (xo, xd, _) = read(ic, xi)?;
        if xo == prog && xd.len() > arc::ENTRIES {
            let (o, m) = arc::bit(arc::TOMBSTONE, bell);
            if xd[o] & m != 0 {
                return Err(e(ARCHIVED));
            }
        }
        if !verified {
            verify(&b)?;
            verified = true;
        }
        init_with_seed(ic, 0, ai, &season, &seed, ban::SIZE, 0)?;
        let mut d = vec![0u8; ban::SIZE];
        header(&mut d, ban::MAGIC, season.id, false);
        put(&mut d, ban::BELL, bell.to_le_bytes());
        d[ban::REGION] = r;
        d[ban::NET] = 2;
        put(&mut d, ban::ROUND, b.round.to_le_bytes());
        put(&mut d, ban::A, now.to_le_bytes());
        put(&mut d, ban::SLOT, slot.to_le_bytes());
        put(&mut d, ban::SIG48, b.sig48);
        put(&mut d, ban::RENT_TO, payer.to_bytes());
        write(ic, ai, &d)?;
        log_ps2(ic, &anchor_body(bell, r, b.round, now, slot, ben));
    }
    Ok(())
}

fn post_seed(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 330_000)?;
    let region = c.u8()?;
    let bell = c.u32()?;
    let nonce = c.u8()?;
    let b = beacon_arg(c)?;
    let _ben = c.arr::<32>()?;
    c.done()?;
    if n_accounts(ic)? != 6 || !is_signer(ic, 0)? {
        return Err(e(BAD_ACCOUNT));
    }
    let (season, s) = season_at(ic, 1)?;
    let prog = program_id(ic)?;
    let aseed = seed_str(
        fclient::addr::SeedKind::BellAnchor,
        &fclient::addr::raw_anchor(bell, region),
    );
    if fclient::addr::with_seed(&season.key, &aseed, &prog) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    let (ao, ad, _) = read(ic, 2)?;
    if ao != prog || ad.len() != ban::SIZE || get::<8>(&ad, 0) != ban::MAGIC {
        return Err(e(NO_ANCHOR));
    }
    let cseed = seed_str(
        fclient::addr::SeedKind::SeedCache,
        &fclient::addr::raw_seed_cache(bell, region, nonce),
    );
    if fclient::addr::with_seed(&season.key, &cseed, &prog) != key(ic, 3)? {
        return Err(e(BAD_ADDRESS));
    }
    let (co, cd, _) = read(ic, 3)?;
    if !is_absent(&co, &cd) {
        return Ok(()); // present: success, no-op
    }
    let a = i64_at(&ad, ban::A);
    if b.round != season_clock(&s).seed_round(bell, a) {
        return Err(e(WRONG_ROUND));
    }
    let seed = verify(&b)?;
    init_with_seed(ic, 0, 3, &season, &cseed, sdc::SIZE, 0)?;
    let (slot, _) = clock(ic)?;
    let mut d = vec![0u8; sdc::SIZE];
    header(&mut d, sdc::MAGIC, season.id, false);
    put(&mut d, sdc::BELL, bell.to_le_bytes());
    d[sdc::REGION] = region;
    d[sdc::NONCE] = nonce;
    put(&mut d, sdc::ROUND, b.round.to_le_bytes());
    put(&mut d, sdc::SEED, seed);
    put(&mut d, sdc::ANCHOR_KEY, key(ic, 2)?.to_bytes());
    put(&mut d, sdc::A, a.to_le_bytes());
    put(&mut d, sdc::SLOT, slot.to_le_bytes());
    put(&mut d, sdc::RENT_TO, key(ic, 0)?.to_bytes());
    write(ic, 3, &d)?;
    let mut body = vec![1u8, 51];
    body.extend_from_slice(&bell.to_le_bytes());
    body.extend_from_slice(&bell.to_le_bytes());
    body.push(region);
    body.push(nonce);
    body.extend_from_slice(&b.round.to_le_bytes());
    body.extend_from_slice(&seed);
    body.extend_from_slice(&a.to_le_bytes());
    body.push(0);
    log_ps2(ic, &body);
    Ok(())
}

fn post_beacon(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 330_000)?;
    let region = c.u8()?;
    let b = beacon_arg(c)?;
    c.done()?;
    let (season, _) = season_at(ic, 1)?;
    let prog = program_id(ic)?;
    let seed = seed_str(fclient::addr::SeedKind::BeaconLog, &[region]);
    if fclient::addr::with_seed(&season.key, &seed, &prog) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    let (o, mut d, _) = read(ic, 2)?;
    if o != prog || d.len() != blg::SIZE {
        return Err(e(BAD_ACCOUNT));
    }
    if b.round <= u64_at(&d, blg::LATEST_ROUND) {
        return Err(e(WRONG_ROUND));
    }
    verify(&b)?;
    let (slot, now) = clock(ic)?;
    put(&mut d, blg::LATEST_ROUND, b.round.to_le_bytes());
    put(&mut d, blg::POSTED_TS, now.to_le_bytes());
    put(&mut d, blg::POSTED_SLOT, slot.to_le_bytes());
    put(&mut d, blg::SIG48, b.sig48);
    put(&mut d, blg::BENEFICIARY, key(ic, 0)?.to_bytes());
    write(ic, 2, &d)?;
    let mut body = vec![1u8, 52];
    let bell = season_clock(&read(ic, 1)?.1)
        .bell_at(now)
        .unwrap_or(u32::MAX);
    body.extend_from_slice(&bell.to_le_bytes());
    body.push(region);
    body.extend_from_slice(&b.round.to_le_bytes());
    body.push(0);
    log_ps2(ic, &body);
    Ok(())
}

/// Closes a program account: data to zero, owner System, lamports to `to`.
fn close(ic: &InvokeContext, acct: u16, to: u16) -> R<()> {
    let c = ic.transaction_context.get_current_instruction_context()?;
    let lamports = {
        let mut a = c.try_borrow_instruction_account(acct)?;
        let l = a.get_lamports();
        a.set_lamports(0)?;
        a.set_data_length(0)?;
        a.set_owner(fclient::addr::system_program().as_ref())?;
        l
    };
    let mut b = c.try_borrow_instruction_account(to)?;
    b.checked_add_lamports(lamports)
}

fn archive_anchors(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 40_000)?;
    let region = c.u8()?;
    let part = c.u32()?;
    let n = c.u8()? as usize;
    let mut bells = vec![];
    for _ in 0..n {
        bells.push(c.u32()?);
    }
    c.done()?;
    if n == 0 || n > 8 || n_accounts(ic)? != 4 + 3 * n as u16 || !is_signer(ic, 0)? {
        return Err(e(BAD_ACCOUNT));
    }
    let (season, s) = season_at(ic, 1)?;
    let prog = program_id(ic)?;
    let aseed = seed_str(
        fclient::addr::SeedKind::AnchorArchive,
        &fclient::addr::raw_archive(region, part),
    );
    if fclient::addr::with_seed(&season.key, &aseed, &prog) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    let (ao, ad, _) = read(ic, 2)?;
    if is_absent(&ao, &ad) {
        init_with_seed(ic, 0, 2, &season, &aseed, arc::SIZE, 0)?;
        let mut d = vec![0u8; arc::SIZE];
        header(&mut d, arc::MAGIC, season.id, false);
        d[arc::REGION] = region;
        put(&mut d, arc::PART, part.to_le_bytes());
        put(&mut d, arc::RENT_TO, key(ic, 0)?.to_bytes());
        write(ic, 2, &d)?;
    }
    let (_, mut arch, _) = read(ic, 2)?;
    let (_, now) = clock(ic)?;
    let sc = season_clock(&s);
    for (j, &b) in bells.iter().enumerate() {
        let (ai, ci, ri) = (4 + 3 * j as u16, 5 + 3 * j as u16, 6 + 3 * j as u16);
        if arc::part_of(b) != part {
            return Err(e(BAD_DATA));
        }
        let seed = seed_str(
            fclient::addr::SeedKind::BellAnchor,
            &fclient::addr::raw_anchor(b, region),
        );
        if fclient::addr::with_seed(&season.key, &seed, &prog) != key(ic, ai)? {
            return Err(e(BAD_ADDRESS));
        }
        let (o, d, _) = read(ic, ai)?;
        if o != prog || d.len() != ban::SIZE {
            return Err(e(NO_ANCHOR));
        }
        let a = i64_at(&d, ban::A);
        if now < a + sc.archive_after as i64 {
            return Err(e(13)); // TooEarly
        }
        if get::<32>(&d, ban::RENT_TO) != key(ic, ri)?.to_bytes() {
            return Err(e(BAD_ACCOUNT));
        }
        let (co, cd, _) = read(ic, ci)?;
        if co != prog
            || cd.len() != sdc::SIZE
            || get::<32>(&cd, sdc::ANCHOR_KEY) != key(ic, ai)?.to_bytes()
            || u64_at(&cd, sdc::ROUND) != sc.seed_round(b, a)
        {
            return Err(e(54)); // SeedNotReady
        }
        // Tombstone and archived bits first, then the entry, then the close.
        for base in [arc::TOMBSTONE, arc::ARCHIVED] {
            let (o, m) = arc::bit(base, b);
            arch[o] |= m;
        }
        let eo = arc::entry(b);
        let a_off = (a - fclient::clock::bell_end(sc.genesis_ts, b)) as u32;
        put(&mut arch, eo + ent::A_OFF, a_off.to_le_bytes());
        put(&mut arch, eo + ent::SEED, get::<32>(&cd, sdc::SEED));
        put(&mut arch, eo + ent::SIG, get::<48>(&d, ban::SIG48));
        write(ic, 2, &arch)?;
        close(ic, ai, ri)?;
    }
    Ok(())
}

fn close_seed_cache(ic: &mut InvokeContext, c: &mut Cursor) -> R<()> {
    consume(ic, 5_000)?;
    let bell = c.u32()?;
    let region = c.u8()?;
    let nonce = c.u8()?;
    c.done()?;
    let (season, _) = season_at(ic, 1)?;
    let prog = program_id(ic)?;
    let cseed = seed_str(
        fclient::addr::SeedKind::SeedCache,
        &fclient::addr::raw_seed_cache(bell, region, nonce),
    );
    if fclient::addr::with_seed(&season.key, &cseed, &prog) != key(ic, 2)? {
        return Err(e(BAD_ADDRESS));
    }
    let (co, cd, _) = read(ic, 2)?;
    if co != prog || cd.len() != sdc::SIZE {
        return Err(e(BAD_ACCOUNT));
    }
    let (xo, xd, _) = read(ic, 3)?;
    let (o, m) = arc::bit(arc::ARCHIVED, bell);
    if xo != prog || xd.len() != arc::SIZE || xd[o] & m == 0 {
        return Err(e(13));
    }
    if get::<32>(&cd, sdc::RENT_TO) != key(ic, 4)?.to_bytes() {
        return Err(e(BAD_ACCOUNT));
    }
    close(ic, 2, 4)
}
