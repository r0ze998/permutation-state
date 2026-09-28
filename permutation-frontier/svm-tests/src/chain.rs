//! The SVM, the program under test, sending with the client's compute-budget
//! profile, **SIMD-0186 loaded-data enforcement** (I-45), accounts and the
//! clock.
//!
//! **The program.** The binary comes from `PSF_SO` (release), `PSF_SO_TEST_BEACON`
//! (feature `test-beacon`, I-53), `PSF_SO_TRACE` (feature `trace`) or
//! `PSF_SO_ORACLE` (feature `oracle`), else from the first build found under
//! the repository (`run.sh` sets the variables after building). It is
//! deployed under LoaderV3 exactly as a real deploy lays it out: a Program
//! account pointing at a ProgramData account of `45 + max_len` bytes with
//! `max_len = round_up(1.25 × .so, 4 KiB)` (§10.1) and an upgrade authority
//! (AnnounceSeason reads it, I-51).
//!
//! **Loaded data (I-45).** LiteSVM 0.16 counts the listed accounts against
//! a transaction's `SetLoadedAccountsDataSizeLimit` but not the LoaderV3
//! ProgramData of the invoked program, and a load failure charges no fee
//! (W1-F measured both; `chain_control_*` below re-proves them). The harness
//! therefore computes SIMD-0186's loaded size itself before execution —
//! Σ(data + 64) over the message's existing accounts (an absent account
//! and the instructions sysvar count 0; builtins at the validator's data
//! length) plus, once, the ProgramData (data + 64) of every invoked
//! LoaderV3 program not listed — and fails a
//! transaction above its limit with `MaxLoadedAccountsDataSizeExceeded`,
//! **fee charged**, as the runtime does. The rule was checked against
//! `solana-test-validator 3.1.9` (W2-B drill, `tests/drill.rs`).
//!
//! **Time.** `set_time` rewrites the Clock sysvar (`unix_timestamp`, and
//! the slot plus one) and expires the blockhash, so two identical
//! transactions are two transactions. Rule times come from the Clock only
//! (§5.1).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fclient::fees;
use fclient::tx;
use frontier_abi::budgets;
use frontier_abi::error::FrontierError;
use frontier_abi::tags::Ix;
use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::Transaction;

use crate::sha256;

/// Mainnet rent per byte (incl. the 128-B overhead): `rent(n) = (128 + n) × 5,080` (§4.2).
pub const LAMPORTS_PER_BYTE: u64 = 5_080;
/// SIMD-0186 per-account base.
pub const ACCOUNT_BASE: u64 = 64;
/// LoaderV3 ProgramData metadata before the ELF.
pub const PROGRAMDATA_META: usize = 45;
/// The runtime's loaded-data ceiling without a limit instruction.
pub const RUNTIME_MAX_LOADED: u32 = 64 * 1024 * 1024;
/// One SIMD-0186 page.
pub const PAGE: u32 = 32_768;
/// The Clock at a new chain: 2026-09-20T00:00:00Z. The SP-V2 fixture rounds
/// (32,551,361 … 32,553,016) are published ≈ 2026-09-25, after it.
pub const T0: i64 = 1_789_862_400;
/// The file the SBF build writes.
pub const SO_NAME: &str = "permutation_frontier.so";
/// Marker strings (§3.2, I-53): a release binary carries none of them.
pub const MARKER_TEST_BEACON: &[u8] = b"PSF_TEST_BEACON_BUILD";
pub const MARKER_TRACE: &[u8] = b"PSF_TRACE_BUILD";
pub const MARKER_ORACLE: &[u8] = b"PSF_ORACLE_BUILD";

/// Which build of the program a chain runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Build {
    /// `scripts/build-frontier.sh`: the deployable binary (real quicknet only).
    Release,
    /// `--features test-beacon`: pins the test key (I-53); never deployable.
    TestBeacon,
    /// `--features trace`: CU and heap markers; never deployable.
    Trace,
    /// `--features oracle`: adds ResolveClash (tests); never deployable.
    Oracle,
}

impl Build {
    pub const ALL: [Build; 4] = [
        Build::Release,
        Build::TestBeacon,
        Build::Trace,
        Build::Oracle,
    ];

    /// The environment variable naming the binary.
    pub const fn env(self) -> &'static str {
        match self {
            Build::Release => "PSF_SO",
            Build::TestBeacon => "PSF_SO_TEST_BEACON",
            Build::Trace => "PSF_SO_TRACE",
            Build::Oracle => "PSF_SO_ORACLE",
        }
    }
    /// The output directory `run.sh` builds it into (§3.2).
    pub const fn dir(self) -> &'static str {
        match self {
            Build::Release => "deploy",
            Build::TestBeacon => "deploy-test-beacon",
            Build::Trace => "deploy-trace",
            Build::Oracle => "deploy-oracle",
        }
    }
    /// The marker the build must carry (none for the release).
    pub const fn marker(self) -> Option<&'static [u8]> {
        match self {
            Build::Release => None,
            Build::TestBeacon => Some(MARKER_TEST_BEACON),
            Build::Trace => Some(MARKER_TRACE),
            Build::Oracle => Some(MARKER_ORACLE),
        }
    }
    /// Whether the build verifies test-key beacons (else real quicknet).
    pub const fn test_key(self) -> bool {
        matches!(self, Build::TestBeacon)
    }
}

/// The repository root (independent of the working directory).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Where a build may live, most specific first: the program crate's own
/// target, the root workspace's, and the root config's
/// (`permutation-chain/target`, `.cargo/config.toml`).
pub fn so_candidates(b: Build) -> Vec<PathBuf> {
    let root = repo_root();
    [
        "permutation-frontier/target",
        "target",
        "permutation-chain/target",
    ]
    .iter()
    .map(|t| root.join(t).join(b.dir()).join(SO_NAME))
    .collect()
}

/// The binary of `b`: its variable, else the newest candidate that exists.
pub fn so_path(b: Build) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(b.env()) {
        return Some(PathBuf::from(p));
    }
    so_candidates(b)
        .into_iter()
        .filter_map(|p| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((t, p))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Reads the binary of `b` (panics with the way to build it).
pub fn load_so(b: Build) -> (PathBuf, Vec<u8>) {
    let path = so_path(b).unwrap_or_else(|| {
        panic!(
            "no {b:?} build of permutation-frontier ({} unset; looked in {:?}); run permutation-frontier/svm-tests/run.sh",
            b.env(),
            so_candidates(b)
        )
    });
    let so = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run permutation-frontier/svm-tests/run.sh)",
            path.display()
        )
    });
    report_binary(b, &path, &so);
    (path, so)
}

/// Whether `so` contains `needle`.
pub fn contains(so: &[u8], needle: &[u8]) -> bool {
    so.windows(needle.len()).any(|w| w == needle)
}

fn report_binary(b: Build, path: &Path, so: &[u8]) {
    static ONCE: OnceLock<std::sync::Mutex<Vec<Build>>> = OnceLock::new();
    let seen = ONCE.get_or_init(|| std::sync::Mutex::new(vec![]));
    let mut seen = seen.lock().unwrap_or_else(|e| e.into_inner());
    if seen.contains(&b) {
        return;
    }
    seen.push(b);
    let age = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| format!("{}s ago", d.as_secs()))
        .unwrap_or_default();
    eprintln!(
        "svm-tests: {b:?} {} ({} B, sha256 {}, built {age})",
        path.display(),
        so.len(),
        hex::encode(sha256(&[so]))
    );
}

/// The program id the harness deploys at: `PSF_PROGRAM_ID` (base58), else a
/// fixed test key. The program derives every address from the id it runs
/// under (§4.1), so any id works; a fixed one keeps addresses reproducible.
pub fn program_id() -> Address {
    match std::env::var("PSF_PROGRAM_ID") {
        Ok(s) => s.parse().expect("PSF_PROGRAM_ID: base58 address"),
        Err(_) => Address::new_from_array(sha256(&[b"PSF-SVM-TESTS program id v1"])),
    }
}

/// `--max-len = round_up(1.25 × .so, 4,096)` (§10.1).
pub fn max_len_for(so_len: usize) -> usize {
    fees::deploy_max_len(so_len as u64) as usize
}

/// LoaderV3 (`BPFLoaderUpgradeab1e11111111111111111111111`).
pub fn loader_v3() -> Address {
    Address::new_from_array(frontier_abi::prologue::ids::LOADER_V3)
}

/// The ProgramData address of `program` under LoaderV3.
pub fn programdata_of(program: &Address) -> Address {
    fclient::addr::programdata(program)
}

/// The compute-budget prefix of a transaction (§5.5, §10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    /// `SetComputeUnitLimit` (none: the runtime default).
    pub cu_limit: Option<u32>,
    /// `SetComputeUnitPrice` in µlamports (0: omitted).
    pub cu_price: u64,
    /// `SetLoadedAccountsDataSizeLimit` (none: the runtime's 64 MiB).
    pub loaded_limit: Option<u32>,
    /// `RequestHeapFrame` (none: 32 KiB).
    pub heap: Option<u32>,
}

impl Profile {
    /// No compute-budget instruction at all.
    pub const NONE: Profile = Profile {
        cu_limit: None,
        cu_price: 0,
        loaded_limit: None,
        heap: None,
    };

    /// What the client sends `ix` with (§5.5, §10.2): the budgets table's
    /// CU limit and `L(kind)` for the deployed programdata length. This is
    /// the profile the G1 gate (W5-A) measures against.
    pub fn client(ix: Ix, programdata_len: u32) -> Profile {
        Profile {
            cu_limit: Some(budgets::budget(ix).cu_limit),
            cu_price: 0,
            loaded_limit: Some(loaded_limit(ix, programdata_len)),
            heap: None,
        }
    }

    /// The keeper's retry ladder top (I-50): 1.4M CU, `L(kind)`, no heap
    /// frame. Functional gates (G2–G12) send with it, so a CU budget miss
    /// (G1, W5-A) never masks the property under test.
    pub fn ladder(ix: Ix, programdata_len: u32) -> Profile {
        Profile {
            cu_limit: Some(budgets::CU_LADDER_MAX),
            cu_price: 0,
            loaded_limit: Some(loaded_limit(ix, programdata_len)),
            heap: None,
        }
    }

    pub fn with_loaded(mut self, l: u32) -> Profile {
        self.loaded_limit = Some(l);
        self
    }
    pub fn with_cu(mut self, cu: u32) -> Profile {
        self.cu_limit = Some(cu);
        self
    }
    pub fn with_price(mut self, micro: u64) -> Profile {
        self.cu_price = micro;
        self
    }
    pub fn with_heap(mut self, bytes: u32) -> Profile {
        self.heap = Some(bytes);
        self
    }

    pub fn instructions(&self) -> Vec<Instruction> {
        let mut v = vec![];
        if let Some(l) = self.cu_limit {
            v.push(tx::set_compute_unit_limit(l));
        }
        if self.cu_price > 0 {
            v.push(tx::set_compute_unit_price(self.cu_price));
        }
        if let Some(l) = self.loaded_limit {
            v.push(tx::set_loaded_accounts_data_size_limit(l));
        }
        if let Some(h) = self.heap {
            v.push(tx::request_heap_frame(h));
        }
        v
    }
}

/// `L(kind)` the client requests for `ix` at `programdata_len` (§10.1,
/// I-45): the kernel formula over the kind's worst account set
/// (`frontier_abi::budgets`), never below the 1-MiB working default.
pub fn loaded_limit(ix: Ix, programdata_len: u32) -> u32 {
    budgets::loaded_limit_for(ix, programdata_len).max(budgets::LOADED_LIMIT_WORKING_DEFAULT)
}

/// A transaction that landed.
#[derive(Clone, Debug)]
pub struct Landed {
    pub signature: Signature,
    pub cu: u64,
    pub fee: u64,
    pub logs: Vec<String>,
    pub return_data: Vec<u8>,
    /// Wire size in bytes.
    pub tx_bytes: usize,
    /// SIMD-0186 loaded size the harness computed.
    pub loaded: u64,
    /// The limit the transaction requested (64 MiB if none).
    pub loaded_limit: u32,
}

/// A transaction that failed (and was charged, unless `charged` is false).
#[derive(Clone, Debug)]
pub struct Fail {
    /// `Custom(code)` if the program (or a program it called) returned one.
    pub code: Option<u32>,
    pub err: String,
    pub logs: Vec<String>,
    pub cu: u64,
    /// The fee taken (0 when the runtime refused it before charging).
    pub fee: u64,
    pub charged: bool,
    pub tx_bytes: usize,
    pub loaded: u64,
    pub loaded_limit: u32,
}

impl Fail {
    /// Whether `program` (base58) failed.
    pub fn failed_in(&self, program: &Address) -> bool {
        let at = format!("Program {program} failed");
        self.logs.iter().any(|l| l.starts_with(&at))
    }
    /// SIMD-0186: the transaction's accounts exceeded its loaded-data limit.
    pub fn loaded_exceeded(&self) -> bool {
        self.err.contains("MaxLoadedAccountsDataSizeExceeded")
    }
}

pub type SendResult = Result<Landed, Fail>;

/// The landed transaction, or a panic with its logs.
#[track_caller]
pub fn expect_lands(r: SendResult, what: &str) -> Landed {
    match r {
        Ok(l) => l,
        Err(f) => panic!(
            "{what}: expected to land, failed with {} (code {:?})\n{}",
            f.err,
            f.code.and_then(FrontierError::from_code).map(|e| e.name()),
            f.logs.join("\n")
        ),
    }
}

/// Asserts that the program itself refused with `e` (a stable §5.4 code).
#[track_caller]
pub fn assert_code(r: SendResult, e: FrontierError) -> Fail {
    let program = program_id();
    match r {
        Ok(l) => panic!(
            "expected {} ({}), landed: {}",
            e.name(),
            e.code(),
            l.logs.join("\n")
        ),
        Err(f) => {
            assert_eq!(
                f.code,
                Some(e.code()),
                "expected {} ({}), got {} / {:?} ({:?})\n{}",
                e.name(),
                e.code(),
                f.err,
                f.code,
                f.code.and_then(FrontierError::from_code).map(|x| x.name()),
                f.logs.join("\n")
            );
            let line = format!(
                "Program {program} failed: custom program error: {:#x}",
                e.code()
            );
            assert!(
                f.logs.iter().any(|l| l == &line),
                "{} not raised by the Frontier program itself:\n{}",
                e.name(),
                f.logs.join("\n")
            );
            f
        }
    }
}

/// Asserts that the program refused (any program code; used where the
/// contract pins the refusal but not its code). A failure outside the
/// Frontier program (a runtime or builder error) is not a refusal
/// (integ-W2 review of W2-B).
#[track_caller]
pub fn assert_refused(r: SendResult, what: &str) -> Fail {
    match r {
        Ok(l) => panic!("{what}: expected a refusal, landed: {}", l.logs.join("\n")),
        Err(f) => {
            assert!(
                !f.loaded_exceeded(),
                "{what}: refused for loaded data, not by the program: {f:?}"
            );
            assert!(
                f.code.is_some(),
                "{what}: failed without a program error code (not a program refusal): {f:?}"
            );
            f
        }
    }
}

/// Asserts a SIMD-0186 refusal with the fee charged.
#[track_caller]
pub fn assert_loaded_exceeded(r: SendResult) -> Fail {
    match r {
        Ok(l) => panic!(
            "expected MaxLoadedAccountsDataSizeExceeded ({} B > limit {} B?), landed",
            l.loaded, l.loaded_limit
        ),
        Err(f) => {
            assert!(f.loaded_exceeded(), "expected a loaded-data refusal: {f:?}");
            assert!(
                f.charged && f.fee > 0,
                "the fee is charged (SIMD-0186): {f:?}"
            );
            f
        }
    }
}

pub struct Chain {
    pub svm: LiteSVM,
    pub build: Build,
    pub program: Address,
    pub programdata: Address,
    pub so_len: usize,
    /// The deployed `max_len` (ProgramData data = 45 + max_len).
    pub max_len: usize,
    /// Signs AnnounceSeason (I-51).
    pub upgrade_authority: Keypair,
    pub now: i64,
    pub slot: u64,
    /// The harness's SIMD-0186 check (on by default; the control turns it off).
    pub enforce_loaded: bool,
}

impl Chain {
    /// The release binary (`PSF_SO`).
    pub fn release() -> Chain {
        Chain::new(Build::Release)
    }
    /// The test-beacon binary (`PSF_SO_TEST_BEACON`, I-53).
    pub fn test_beacon() -> Chain {
        Chain::new(Build::TestBeacon)
    }

    pub fn new(b: Build) -> Chain {
        let (_, so) = load_so(b);
        Chain::with_program(b, &so, program_id())
    }

    /// A chain with `so` deployed at `program` (the control deploys other ELFs).
    pub fn with_program(b: Build, so: &[u8], program: Address) -> Chain {
        let mut c = Chain::bare(b);
        c.program = program;
        c.programdata = programdata_of(&program);
        c.so_len = so.len();
        c.max_len = max_len_for(so.len());
        let auth = c.upgrade_authority.pubkey();
        c.deploy(program, so, c.max_len, Some(auth));
        c
    }

    /// LiteSVM with mainnet rent, the Clock at `T0` and nothing deployed.
    pub fn bare(b: Build) -> Chain {
        let mut svm = LiteSVM::new()
            .with_sigverify(true)
            .with_log_bytes_limit(Some(100_000));
        let mut rent: solana_rent::Rent = svm.get_sysvar();
        rent.lamports_per_byte = LAMPORTS_PER_BYTE;
        #[allow(deprecated)]
        {
            rent.exemption_threshold = 1.0f64.to_le_bytes();
        }
        svm.set_sysvar(&rent);
        let mut c = Chain {
            svm,
            build: b,
            program: Address::default(),
            programdata: Address::default(),
            so_len: 0,
            max_len: 0,
            upgrade_authority: crate::keypair(b"upgrade-authority"),
            now: T0,
            slot: 1,
            enforce_loaded: true,
        };
        c.set_time(T0);
        c
    }

    /// Deploys `so` under LoaderV3 at `program` with ProgramData of
    /// `max_len` bytes (zero padded, as a `--max-len` deploy) and the given
    /// upgrade authority.
    pub fn deploy(&mut self, program: Address, so: &[u8], max_len: usize, auth: Option<Address>) {
        let pd = programdata_of(&program);
        let max_len = max_len.max(so.len());
        let mut data = vec![0u8; PROGRAMDATA_META + max_len];
        data[..4].copy_from_slice(&3u32.to_le_bytes()); // ProgramData
        data[4..12].copy_from_slice(&self.slot.to_le_bytes());
        if let Some(a) = auth {
            data[12] = 1;
            data[13..45].copy_from_slice(a.as_ref());
        }
        data[PROGRAMDATA_META..PROGRAMDATA_META + so.len()].copy_from_slice(so);
        let lamports = self.rent(data.len());
        self.svm
            .set_account(
                pd,
                Account {
                    lamports,
                    data,
                    owner: loader_v3(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("programdata");
        let mut pdata = vec![0u8; 36];
        pdata[..4].copy_from_slice(&2u32.to_le_bytes()); // Program
        pdata[4..].copy_from_slice(pd.as_ref());
        let lamports = self.rent(36);
        self.svm
            .set_account(
                program,
                Account {
                    lamports,
                    data: pdata,
                    owner: loader_v3(),
                    executable: true,
                    rent_epoch: 0,
                },
            )
            .expect("program account (the ELF must load)");
    }

    /// The deployed ProgramData data length (`45 + max_len`), the
    /// `programdata_len` of §10.1's formula being `max_len`.
    pub fn programdata_len(&self) -> u32 {
        self.max_len as u32
    }

    /// An independent copy.
    pub fn fork(&self) -> Chain {
        Chain {
            svm: self.svm.clone(),
            build: self.build,
            program: self.program,
            programdata: self.programdata,
            so_len: self.so_len,
            max_len: self.max_len,
            upgrade_authority: self.upgrade_authority.insecure_clone(),
            now: self.now,
            slot: self.slot,
            enforce_loaded: self.enforce_loaded,
        }
    }

    // ------------------------------------------------------------ time

    /// Rewrites the Clock (`unix_timestamp = t`, slot + 1) and expires the blockhash.
    pub fn set_time(&mut self, t: i64) {
        self.now = t;
        self.slot += 1;
        let mut c: solana_clock::Clock = self.svm.get_sysvar();
        c.unix_timestamp = t;
        c.slot = self.slot;
        self.svm.set_sysvar(&c);
        self.svm.expire_blockhash();
    }

    pub fn advance(&mut self, seconds: i64) {
        let t = self.now + seconds;
        self.set_time(t);
    }

    /// Moves only the slot (same Clock time), e.g. for evidence tests.
    pub fn next_slot(&mut self) {
        let t = self.now;
        self.set_time(t);
    }

    // ------------------------------------------------------------ keys and lamports

    /// `rent(n) = (128 + n) × 5,080` (the Rent sysvar the harness installs).
    pub fn rent(&self, space: usize) -> u64 {
        self.svm.minimum_balance_for_rent_exemption(space)
    }

    pub fn airdrop(&mut self, k: &Address, lamports: u64) {
        self.svm.airdrop(k, lamports).expect("airdrop");
    }

    /// A key with `sol` SOL.
    pub fn funded(&mut self, label: &[u8], sol: u64) -> Keypair {
        let k = crate::keypair(label);
        self.airdrop(&k.pubkey(), sol * 1_000_000_000);
        k
    }

    // ------------------------------------------------------------ accounts

    /// The account, if it exists (lamports or data).
    pub fn account(&self, k: &Address) -> Option<Account> {
        self.svm
            .get_account(k)
            .filter(|a| a.lamports > 0 || !a.data.is_empty())
    }
    pub fn data(&self, k: &Address) -> Vec<u8> {
        self.account(k).map(|a| a.data).unwrap_or_default()
    }
    pub fn lamports(&self, k: &Address) -> u64 {
        self.account(k).map_or(0, |a| a.lamports)
    }
    pub fn owner(&self, k: &Address) -> Option<Address> {
        self.account(k).map(|a| a.owner)
    }
    /// §4.1 absence: owner = System and no data (lamports ignored).
    pub fn is_absent(&self, k: &Address) -> bool {
        match self.account(k) {
            None => true,
            Some(a) => a.owner == Address::default() && a.data.is_empty(),
        }
    }

    /// Creates or replaces an account.
    pub fn put(&mut self, k: Address, owner: Address, data: Vec<u8>, lamports: u64) {
        self.svm
            .set_account(
                k,
                Account {
                    lamports,
                    data,
                    owner,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("set_account");
    }

    /// A program-owned account with rent-exempt lamports.
    pub fn put_program_account(&mut self, k: Address, data: Vec<u8>) {
        let l = self.rent(data.len());
        let p = self.program;
        self.put(k, p, data, l);
    }

    /// Pre-funds an address with `lamports` and no data (still absent, §4.1).
    pub fn prefund(&mut self, k: &Address, lamports: u64) {
        let have = self.lamports(k);
        self.put(*k, Address::default(), vec![], have + lamports);
    }

    /// Sets an existing account's lamports (owner and data kept).
    pub fn edit_lamports(&mut self, k: &Address, lamports: u64) {
        let mut a = self.svm.get_account(k).expect("account exists");
        a.lamports = lamports;
        self.svm.set_account(*k, a).expect("set_account");
    }

    /// Replaces an existing account's data (owner and lamports kept).
    pub fn set_data(&mut self, k: &Address, data: Vec<u8>) {
        let mut a = self.svm.get_account(k).expect("account exists");
        a.data = data;
        self.svm.set_account(*k, a).expect("set_account");
    }
    /// Edits an existing account's data in place.
    pub fn edit(&mut self, k: &Address, f: impl FnOnce(&mut [u8])) {
        let mut d = self.data(k);
        f(&mut d);
        self.set_data(k, d);
    }
    pub fn set_owner(&mut self, k: &Address, owner: Address) {
        let mut a = self.svm.get_account(k).expect("account exists");
        a.owner = owner;
        self.svm.set_account(*k, a).expect("set_account");
    }
    /// Removes an account entirely (absent, no lamports).
    pub fn remove(&mut self, k: &Address) {
        self.put(*k, Address::default(), vec![], 0);
    }

    // ------------------------------------------------------------ loaded data (I-45)

    /// SIMD-0186 loaded size of `msg` against the current state: Σ(data +
    /// 64) over the message's existing accounts (the instructions sysvar
    /// and absent accounts count 0) plus the ProgramData (data + 64) of
    /// every invoked LoaderV3 program not already listed, each once.
    pub fn loaded_size(&self, msg: &Message) -> u64 {
        let ix_sysvar = fclient::addr::instructions_sysvar();
        let mut total = 0u64;
        for k in &msg.account_keys {
            if *k == ix_sysvar {
                continue;
            }
            if let Some(a) = self.svm.get_account(k) {
                if a.lamports > 0 || !a.data.is_empty() || a.executable {
                    total += ACCOUNT_BASE + builtin_data_len(k).unwrap_or(a.data.len()) as u64;
                }
            }
        }
        let mut counted: Vec<Address> = vec![];
        for ci in &msg.instructions {
            let pid = msg.account_keys[ci.program_id_index as usize];
            let Some(p) = self.svm.get_account(&pid) else {
                continue;
            };
            if p.owner != loader_v3() || p.data.len() < 36 || p.data[..4] != 2u32.to_le_bytes() {
                continue;
            }
            let pd = Address::new_from_array(p.data[4..36].try_into().expect("32"));
            if msg.account_keys.contains(&pd) || counted.contains(&pd) {
                continue;
            }
            counted.push(pd);
            if let Some(a) = self.svm.get_account(&pd) {
                total += ACCOUNT_BASE + a.data.len() as u64;
            }
        }
        total
    }

    /// The smallest page-multiple limit `msg` loads under.
    pub fn loaded_need_pages(&self, msg: &Message) -> u32 {
        let n = self.loaded_size(msg);
        (n.div_ceil(PAGE as u64) * PAGE as u64) as u32
    }

    // ------------------------------------------------------------ sending

    /// Builds and signs a transaction: `profile`'s compute-budget prefix,
    /// then `ixs`; `signers[0]` pays; signers the message does not need
    /// are ignored.
    pub fn transaction(
        &self,
        profile: &Profile,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> Transaction {
        let mut all = profile.instructions();
        all.extend_from_slice(ixs);
        let payer = signers[0].pubkey();
        let bh = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(&all, Some(&payer), &bh);
        let required = &msg.account_keys[..msg.header.num_required_signatures as usize];
        let mut keys: Vec<&Keypair> = vec![];
        for k in signers {
            if required.contains(&k.pubkey()) && !keys.iter().any(|x| x.pubkey() == k.pubkey()) {
                keys.push(k);
            }
        }
        let mut t = Transaction::new_unsigned(msg);
        t.try_sign(&keys, bh)
            .expect("a keypair for every signer of the transaction");
        t
    }

    /// Sends `ixs` with `profile`.
    pub fn send_with(
        &mut self,
        profile: &Profile,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> SendResult {
        let t = self.transaction(profile, ixs, signers);
        self.submit(t)
    }

    /// Sends with the keeper's retry-ladder profile for the first Frontier
    /// instruction's kind (1.4M CU, `L(kind)`), see [`Profile::ladder`].
    pub fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> SendResult {
        let p = self.profile_of(ixs, Profile::ladder);
        self.send_with(&p, ixs, signers)
    }

    /// Sends with the client profile (budget CU limit, `L(kind)`).
    pub fn send_client(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> SendResult {
        let p = self.profile_of(ixs, Profile::client);
        self.send_with(&p, ixs, signers)
    }

    /// `f(kind, programdata_len)` for the Frontier instruction of `ixs`
    /// with the largest `L(kind)` (runtime default if none).
    pub fn profile_of(&self, ixs: &[Instruction], f: fn(Ix, u32) -> Profile) -> Profile {
        ixs.iter()
            .filter(|i| i.program_id == self.program)
            .filter_map(|i| i.data.first().and_then(|t| Ix::from_tag(*t)))
            .map(|ix| f(ix, self.programdata_len()))
            .max_by_key(|p| p.loaded_limit)
            .unwrap_or(Profile::NONE)
    }

    /// Executes a signed transaction with the SIMD-0186 check.
    pub fn submit(&mut self, t: Transaction) -> SendResult {
        let tx_bytes = bincode::serialize(&t).map(|v| v.len()).unwrap_or(0);
        let loaded = self.loaded_size(&t.message);
        let loaded_limit = tx::parse_budget(&t.message).effective_loaded_limit();
        if self.enforce_loaded && loaded > loaded_limit as u64 {
            let fee = tx::fee_lamports(&t.message);
            let payer = t.message.account_keys[0];
            let charged = self.charge(&payer, fee);
            self.svm.expire_blockhash();
            return Err(Fail {
                code: None,
                err: "MaxLoadedAccountsDataSizeExceeded".into(),
                logs: vec![format!(
                    "svm-tests: loaded {loaded} B > limit {loaded_limit} B (SIMD-0186)"
                )],
                cu: 0,
                fee: if charged { fee } else { 0 },
                charged,
                tx_bytes,
                loaded,
                loaded_limit,
            });
        }
        let kinds = self.frontier_kinds(&t.message);
        let locks = t.message.account_keys.len();
        let r = self.svm.send_transaction(t);
        self.svm.expire_blockhash();
        if let Ok(m) = &r {
            self.log_cu(
                &kinds,
                m.compute_units_consumed,
                tx_bytes,
                locks,
                loaded,
                &m.logs,
            );
        }
        match r {
            Ok(m) => Ok(Landed {
                signature: m.signature,
                cu: m.compute_units_consumed,
                fee: m.fee,
                logs: m.logs,
                return_data: m.return_data.data,
                tx_bytes,
                loaded,
                loaded_limit,
            }),
            Err(f) => {
                let err = format!("{:?}", f.err);
                let code = custom_code(&err);
                // G13 (§5.4, §13.3): with RELEASE_CHECK=1 no path may
                // return NotImplemented (99), whatever the test asserts.
                if code == Some(99) && std::env::var("RELEASE_CHECK").is_ok_and(|v| v == "1") {
                    panic!("RELEASE_CHECK=1: a path returned NotImplemented (99): {err}");
                }
                Err(Fail {
                    code,
                    charged: f.meta.fee > 0,
                    fee: f.meta.fee,
                    err,
                    logs: f.meta.logs,
                    cu: f.meta.compute_units_consumed,
                    tx_bytes,
                    loaded,
                    loaded_limit,
                })
            }
        }
    }

    /// The Frontier instruction kinds of `msg`, in order.
    fn frontier_kinds(&self, msg: &Message) -> Vec<Ix> {
        msg.instructions
            .iter()
            .filter(|i| msg.account_keys.get(i.program_id_index as usize) == Some(&self.program))
            .filter_map(|i| i.data.first().and_then(|t| Ix::from_tag(*t)))
            .collect()
    }

    /// W5-A (budgets regenerated from G1, §5.5): with `PSF_CU_LOG=<file>`,
    /// every landed transaction with exactly one Frontier instruction
    /// appends `build kind cu tx_bytes locks loaded heap` to the file (the
    /// suite's measured maxima feed `frontier_abi::budgets`; the heap peak
    /// only on the trace build, `-` otherwise).
    fn log_cu(
        &self,
        kinds: &[Ix],
        cu: u64,
        tx_bytes: usize,
        locks: usize,
        loaded: u64,
        logs: &[String],
    ) {
        let Ok(path) = std::env::var("PSF_CU_LOG") else {
            return;
        };
        let [kind] = kinds else {
            return;
        };
        let heap = crate::budget::heap_peak(logs).map_or("-".to_string(), |h| h.to_string());
        let line = format!(
            "{:?} {} {cu} {tx_bytes} {locks} {loaded} {heap}\n",
            self.build,
            kind.name()
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn charge(&mut self, payer: &Address, fee: u64) -> bool {
        let Some(mut a) = self.svm.get_account(payer) else {
            return false;
        };
        if a.lamports < fee {
            return false;
        }
        a.lamports -= fee;
        self.svm.set_account(*payer, a).is_ok()
    }
}

/// Data lengths of builtin program accounts as the validator stores them
/// (W2-B drill on `solana-test-validator 3.1.9`: the System program's data
/// is `system_program`, 14 B; LiteSVM 0.16 stores `solana_system_program`,
/// 21 B, so the harness uses the validator's number).
pub fn builtin_data_len(k: &Address) -> Option<usize> {
    if *k == Address::default() {
        Some(DRILL_SYSTEM_DATA)
    } else if *k == fclient::addr::compute_budget_program() {
        Some(DRILL_COMPUTE_BUDGET_DATA)
    } else {
        None
    }
}

/// System program account data on the validator (drill) [measured].
pub const DRILL_SYSTEM_DATA: usize = 14;
/// ComputeBudget program account data on the validator (drill) [measured].
pub const DRILL_COMPUTE_BUDGET_DATA: usize = 22;

/// `Custom(n)` in a `TransactionError` debug string.
pub fn custom_code(err: &str) -> Option<u32> {
    err.split("Custom(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .and_then(|s| s.parse().ok())
}

/// A transaction's wire size (1,232-B packet limit) with `n_signers` signatures.
pub fn tx_size(ixs: &[Instruction], payer: &Address) -> usize {
    let msg = Message::new(ixs, Some(payer));
    let n = msg.header.num_required_signatures as usize;
    let t = Transaction {
        signatures: vec![Signature::default(); n],
        message: msg,
    };
    bincode::serialize(&t)
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
}

/// Replaces the key of account `i` of `ix` (a forged or substituted account).
pub fn with_account(mut ix: Instruction, i: usize, key: Address) -> Instruction {
    ix.accounts[i].pubkey = key;
    ix
}

/// Flips the writability of account `i` of `ix`.
pub fn with_writable(mut ix: Instruction, i: usize, writable: bool) -> Instruction {
    ix.accounts[i].is_writable = writable;
    ix
}

/// Drops the signer flag of account `i` of `ix`.
pub fn without_signer(mut ix: Instruction, i: usize) -> Instruction {
    ix.accounts[i].is_signer = false;
    ix
}
