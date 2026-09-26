//! The SVM, sending, accounts and the clock.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use borsh::{BorshDeserialize, BorshSerialize};
use litesvm::LiteSVM;
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::Address;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::Transaction;

use crate::error::ChainError;
use crate::state::{load, Chunks};
use crate::{addr, PROGRAM_ID, T0, TOKEN};

/// The heap frame heavy transactions request (`chain.mjs` `HEAP_BYTES`).
pub const HEAP: u32 = 256 * 1024;
/// The CU limit heavy transactions request.
pub const TX_CU: u32 = 1_400_000;
/// The heap frame player transactions request (`chain.mjs` `medium()`).
pub const MEDIUM_HEAP: u32 = 128 * 1024;

/// The compute-budget profiles of the real client
/// (`permutation-gateway/client/src/chain.mjs`). Every test sends with the
/// profile the client uses, so "fits 1.4M CU but not the player's default
/// budget" is caught.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Budget {
    /// No compute-budget instructions: 200k CU, the default 32 KiB heap.
    Light,
    /// `RequestHeapFrame(128 KiB)` only (`medium()`, `player.mjs`).
    Medium,
    /// `SetComputeUnitLimit(1.4M)` + `RequestHeapFrame(256 KiB)` (`heavy()`).
    Heavy,
}

/// The profile the client sends an instruction with, by its tag. A new
/// instruction joins the list its `chain.mjs` builder uses.
pub fn client_budget(tag: u8) -> Budget {
    match tag {
        // StartSeason, GenesisStep, ResolveTick, FinishSeason, SeatMembers,
        // OpenGovernment, LogTickInput, CloseCommits.
        3 | 4 | 7 | 10 | 15 | 16 | 19 | 21 => Budget::Heavy,
        // SubmitGov, CommitOrders, RevealOrders.
        17 | 22 | 23 => Budget::Medium,
        _ => Budget::Light,
    }
}

/// A landed transaction.
#[derive(Debug)]
pub struct Landed {
    pub cu: u64,
    pub logs: Vec<String>,
}

/// A failed transaction: `code` is the `Custom(code)` it failed with, if any.
#[derive(Debug)]
pub struct Fail {
    pub code: Option<u32>,
    pub err: String,
    pub logs: Vec<String>,
}

impl Fail {
    /// Whether `program` (base58) failed (a failing CPI fails its caller too).
    pub fn failed_in(&self, program: &str) -> bool {
        let at = format!("Program {program} failed");
        self.logs.iter().any(|l| l.starts_with(&at))
    }

    /// Whether only the program under test failed: no program it called did.
    pub fn failed_alone(&self) -> bool {
        let ours = format!("Program {PROGRAM_ID} failed");
        self.failed_in(PROGRAM_ID)
            && !self.logs.iter().any(|l| {
                l.starts_with("Program ") && l.contains(" failed") && !l.starts_with(&ours)
            })
    }
}

/// Asserts that the program itself refused with `e` (not a program it called).
#[track_caller]
pub fn assert_err(r: Result<Landed, Fail>, e: ChainError) {
    match r {
        Ok(l) => panic!("expected {} ({}), landed: {:?}", e.name(), e as u32, l.logs),
        Err(f) => {
            assert_eq!(f.code, Some(e as u32), "expected {}: {f:#?}", e.name());
            let line = format!(
                "Program {PROGRAM_ID} failed: custom program error: {:#x}",
                e as u32
            );
            assert!(
                f.logs.contains(&line) && f.failed_alone(),
                "{} not raised by the program itself: {f:#?}",
                e.name()
            );
        }
    }
}

/// Asserts that the SPL Token program refused (a transfer the program asked for).
#[track_caller]
pub fn assert_token_err(r: Result<Landed, Fail>) {
    match r {
        Ok(l) => panic!("expected a token program refusal, landed: {:?}", l.logs),
        Err(f) => assert!(
            f.failed_in(TOKEN),
            "not refused by the token program: {f:#?}"
        ),
    }
}

/// Asserts a non-custom `InstructionError` by name (`IncorrectProgramId`,
/// `MissingRequiredSignature`, …) from the program.
#[track_caller]
pub fn assert_program_err(r: Result<Landed, Fail>, name: &str) {
    match r {
        Ok(l) => panic!("expected {name}, landed: {:?}", l.logs),
        Err(f) => {
            assert!(f.err.contains(name), "expected {name}: {f:#?}");
            assert!(
                f.failed_alone(),
                "{name} not raised by the program itself: {f:#?}"
            );
        }
    }
}

pub struct Chain {
    pub svm: LiteSVM,
    pub program: Address,
    /// The clock's unix time.
    pub now: i64,
    /// The heap frame `Budget::Heavy` requests (`need` forks with others).
    pub heap: u32,
    pub sigverify: bool,
}

/// The program binary under test: `PERMUTATION_CHAIN_SO`, else the
/// SBF build next to this crate (independent of the working directory).
pub fn program_path() -> PathBuf {
    std::env::var_os("PERMUTATION_CHAIN_SO")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../target/deploy/permutation_chain.so"
            )
            .into()
        })
}

fn newest(dir: &Path) -> Option<(SystemTime, PathBuf)> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let found = if p.is_dir() {
            newest(&p)
        } else {
            e.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (t, p))
        };
        if let Some(f) = found {
            if best.as_ref().is_none_or(|b| f.0 > b.0) {
                best = Some(f);
            }
        }
    }
    best
}

/// Once per test process: which binary is tested (path, sha256, mtime), and a
/// warning when it is older than the newest program or rules source (the
/// same rule as `local-stack.mjs`). `run.sh` always rebuilds it first.
fn report_binary(path: &Path, so: &[u8]) {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let built = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let hash: String = Sha256::digest(so)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let age = built
            .and_then(|t| t.elapsed().ok())
            .map(|d| format!("{}s ago", d.as_secs()))
            .unwrap_or_default();
        eprintln!(
            "svm-tests: program {} sha256 {hash} built {age}",
            path.display()
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for src in [
            root.join("../src"),
            root.join("../../permutation-rules/src"),
        ] {
            if let (Some(built), Some((t, p))) = (built, newest(&src)) {
                if t > built {
                    eprintln!(
                        "svm-tests: WARNING the binary is older than {} (stale build: use run.sh)",
                        p.display()
                    );
                }
            }
        }
    });
}

impl Chain {
    /// The program under test with signature verification on.
    pub fn new() -> Self {
        Self::new_opts(true)
    }

    /// `sigverify: false` only for bot-driven flows (session keys without
    /// private keys, `send_as`) and the DLP callback (a PDA signs it by CPI
    /// in reality). Signer checks are always tested with sigverify on.
    pub fn new_opts(sigverify: bool) -> Self {
        let path = program_path();
        let so = std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (run permutation-chain/svm-tests/run.sh, or cargo build-sbf in permutation-chain)",
                path.display()
            )
        });
        report_binary(&path, &so);
        let mut svm = LiteSVM::new()
            .with_log_bytes_limit(Some(10_000))
            .with_sigverify(sigverify);
        let program = addr(PROGRAM_ID);
        svm.add_program(program, &so).unwrap();
        let mut c = Chain {
            svm,
            program,
            now: T0,
            heap: HEAP,
            sigverify,
        };
        c.set_time(T0);
        c
    }

    /// An independent copy (for measuring, or for two continuations).
    pub fn fork(&self) -> Chain {
        Chain {
            svm: self.svm.clone(),
            program: self.program,
            now: self.now,
            heap: self.heap,
            sigverify: self.sigverify,
        }
    }

    /// Replace the program binary, keeping every account (a program upgrade).
    pub fn upgrade(&mut self, so: &[u8]) {
        self.svm.add_program(self.program, so).unwrap();
    }

    /// Rewrites the Clock sysvar, bumps the slot and expires the blockhash.
    pub fn set_time(&mut self, t: i64) {
        self.now = t;
        let mut c: solana_clock::Clock = self.svm.get_sysvar();
        c.unix_timestamp = t;
        c.slot += 1;
        self.svm.set_sysvar(&c);
        self.svm.expire_blockhash();
    }

    pub fn advance(&mut self, seconds: i64) {
        let t = self.now + seconds;
        self.set_time(t);
    }

    /// A new key with 100 SOL.
    pub fn funded(&mut self) -> Keypair {
        let k = Keypair::new();
        self.svm.airdrop(&k.pubkey(), 100_000_000_000).unwrap();
        k
    }

    fn budget_ixs(&self, budget: Budget) -> Vec<Instruction> {
        match budget {
            Budget::Light => vec![],
            Budget::Medium => vec![ComputeBudgetInstruction::request_heap_frame(MEDIUM_HEAP)],
            Budget::Heavy => vec![
                ComputeBudgetInstruction::set_compute_unit_limit(TX_CU),
                ComputeBudgetInstruction::request_heap_frame(self.heap),
            ],
        }
    }

    /// The profile the client would send `ixs` with: the heaviest of the
    /// program's instructions among them.
    pub fn budget_of(&self, ixs: &[Instruction]) -> Budget {
        ixs.iter()
            .filter(|i| i.program_id == self.program)
            .map(|i| i.data.first().map_or(Budget::Light, |t| client_budget(*t)))
            .max()
            .unwrap_or(Budget::Light)
    }

    /// Sends `ixs` with the client's budget profile for them. The first
    /// signer pays; signers the transaction does not need are ignored.
    pub fn send(&mut self, ixs: Vec<Instruction>, signers: &[&Keypair]) -> Result<Landed, Fail> {
        let budget = self.budget_of(&ixs);
        self.send_with(budget, ixs, signers)
    }

    /// Sends `ixs` behind the compute-budget instructions of `budget`.
    pub fn send_with(
        &mut self,
        budget: Budget,
        ixs: Vec<Instruction>,
        signers: &[&Keypair],
    ) -> Result<Landed, Fail> {
        let mut all = self.budget_ixs(budget);
        all.extend(ixs);
        let payer = signers[0].pubkey();
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(&all, Some(&payer), &blockhash);
        let required = &msg.account_keys[..msg.header.num_required_signatures as usize];
        let mut keys: Vec<&Keypair> = Vec::new();
        for k in signers {
            if required.contains(&k.pubkey()) && !keys.iter().any(|x| x.pubkey() == k.pubkey()) {
                keys.push(k);
            }
        }
        let mut tx = Transaction::new_unsigned(msg);
        tx.try_sign(&keys, blockhash)
            .expect("a keypair for every signer of the transaction");
        self.submit(tx)
    }

    /// Sends with the signer flags the metas carry and no real signatures
    /// (needs sigverify off): members whose session keys are only public
    /// keys (the play server's `local_key`) sign this way. `payer` pays.
    pub fn send_as(&mut self, ixs: Vec<Instruction>, payer: &Address) -> Result<Landed, Fail> {
        let budget = self.budget_of(&ixs);
        self.send_as_with(budget, ixs, payer)
    }

    pub fn send_as_with(
        &mut self,
        budget: Budget,
        ixs: Vec<Instruction>,
        payer: &Address,
    ) -> Result<Landed, Fail> {
        assert!(!self.sigverify, "send_as needs Chain::new_opts(false)");
        let mut all = self.budget_ixs(budget);
        all.extend(ixs);
        let msg = Message::new_with_blockhash(&all, Some(payer), &self.svm.latest_blockhash());
        let n = msg.header.num_required_signatures as usize;
        self.submit(Transaction {
            signatures: vec![Signature::default(); n],
            message: msg,
        })
    }

    fn submit(&mut self, tx: Transaction) -> Result<Landed, Fail> {
        let r = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();
        match r {
            Ok(m) => Ok(Landed {
                cu: m.compute_units_consumed,
                logs: m.logs,
            }),
            Err(f) => {
                let err = format!("{:?}", f.err);
                let code = err
                    .split("Custom(")
                    .nth(1)
                    .and_then(|s| s.split(')').next())
                    .and_then(|s| s.parse().ok());
                Err(Fail {
                    code,
                    err,
                    logs: f.meta.logs,
                })
            }
        }
    }

    pub fn ix(
        &self,
        data: &crate::instruction::ChainInstruction,
        metas: Vec<solana_instruction::AccountMeta>,
    ) -> Instruction {
        Instruction::new_with_bytes(self.program, &borsh::to_vec(data).unwrap(), metas)
    }

    // ---- accounts

    /// Creates or replaces an account (rent-exempt lamports).
    pub fn put(&mut self, key: Address, owner: Address, data: Vec<u8>) {
        let lamports = self
            .svm
            .minimum_balance_for_rent_exemption(data.len())
            .max(1);
        self.svm
            .set_account(
                key,
                Account {
                    lamports,
                    data,
                    owner,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
    }

    /// Replaces an existing account's data, keeping its owner and lamports.
    pub fn set_data(&mut self, key: &Address, data: Vec<u8>) {
        let mut a = self.svm.get_account(key).expect("account exists");
        a.data = data;
        self.svm.set_account(*key, a).unwrap();
    }

    /// Changes an existing account's owner.
    pub fn set_owner(&mut self, key: &Address, owner: Address) {
        let mut a = self.svm.get_account(key).expect("account exists");
        a.owner = owner;
        self.svm.set_account(*key, a).unwrap();
    }

    /// Loads a borsh account, lets `f` change it and stores it back in place.
    pub fn edit<T: BorshDeserialize + BorshSerialize>(
        &mut self,
        key: &Address,
        f: impl FnOnce(&mut T),
    ) {
        let mut data = self.data(key);
        let mut v: T = load(&data).unwrap();
        f(&mut v);
        let bytes = borsh::to_vec(&v).unwrap();
        data[..bytes.len()].copy_from_slice(&bytes);
        self.set_data(key, data);
    }

    pub fn data(&self, key: &Address) -> Vec<u8> {
        self.svm
            .get_account(key)
            .map(|a| a.data)
            .unwrap_or_default()
    }

    pub fn owner(&self, key: &Address) -> Option<Address> {
        self.svm
            .get_account(key)
            .filter(|a| a.lamports > 0 || !a.data.is_empty())
            .map(|a| a.owner)
    }

    pub fn load<T: BorshDeserialize>(&self, key: &Address) -> T {
        load(&self.data(key)).unwrap()
    }

    /// Reads and writes the world chunks through the program's own `Chunks`.
    pub fn with_world<R>(&mut self, chunks: &[Address], f: impl FnOnce(&Chunks) -> R) -> R {
        use solana_program::{account_info::AccountInfo, pubkey::Pubkey};
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let mut data: Vec<Vec<u8>> = chunks.iter().map(|k| self.data(k)).collect();
        let mut lamports = vec![0u64; chunks.len()];
        let r = {
            let infos: Vec<AccountInfo> = data
                .iter_mut()
                .zip(lamports.iter_mut())
                .map(|(d, l)| {
                    AccountInfo::new(&key, false, true, l, d.as_mut_slice(), &owner, false)
                })
                .collect();
            f(&Chunks::new(&infos).unwrap())
        };
        for (k, d) in chunks.iter().zip(data) {
            if self.data(k) != d {
                self.set_data(k, d);
            }
        }
        r
    }
}

/// A transaction's size on the wire (the packet limit is 1232 bytes):
/// `ixs` with `payer`, signatures included.
pub fn tx_size(ixs: &[Instruction], payer: &Address) -> usize {
    let msg = Message::new(ixs, Some(payer));
    let n = msg.header.num_required_signatures as usize;
    let tx = Transaction {
        signatures: vec![Signature::default(); n],
        message: msg,
    };
    bincode::serialize(&tx).unwrap().len()
}

/// The largest transaction the network carries.
pub const PACKET_DATA_SIZE: usize = 1232;

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::season::{Params, SeasonFx};

    /// The binary under test was built with overflow checks for the
    /// program's own crate (root `Cargo.toml`, audit WP08): a checked
    /// `+ - *` compiles to a panic with these messages, an unchecked build
    /// has none (`scripts/build-program.sh` refuses such a build too).
    #[test]
    fn the_program_is_built_with_overflow_checks() {
        let so = std::fs::read(program_path()).unwrap();
        let found = ["add", "subtract", "multiply"]
            .iter()
            .filter(|op| {
                let msg = format!("attempt to {op} with overflow");
                so.windows(msg.len()).any(|w| w == msg.as_bytes())
            })
            .count();
        assert!(
            found > 0,
            "{}: no overflow checks",
            program_path().display()
        );
    }

    /// LiteSVM swaps the program mid-season and the season keeps playing
    /// (WP15's upgrade tests use `Chain::upgrade`). With `OLD_SO` (run.sh
    /// `OLD_REF=…`) the season starts on that older binary; without it the
    /// binary under test is swapped for itself.
    #[test]
    fn swap_program_mid_season() {
        let new = std::fs::read(program_path()).unwrap();
        let old = match std::env::var("OLD_SO") {
            Ok(p) => std::fs::read(&p).unwrap_or_else(|e| panic!("OLD_SO {p}: {e}")),
            Err(_) => {
                eprintln!("OLD_SO unset: swapping the binary under test for itself");
                new.clone()
            }
        };
        let mut c = Chain::new();
        c.upgrade(&old);
        let mut s = SeasonFx::create(&mut c, Params::default());
        for i in 0..4 {
            s.register(&mut c, (i % 2) as u16, 0);
        }
        s.genesis(&mut c);
        s.seat_and_open(&mut c);
        s.play_tick(&mut c);
        let before = s.world(&mut c).tick;
        c.upgrade(&new);
        s.play_tick(&mut c);
        assert_eq!(
            s.world(&mut c).tick,
            before + 1,
            "the season plays on after the swap"
        );
    }
}
