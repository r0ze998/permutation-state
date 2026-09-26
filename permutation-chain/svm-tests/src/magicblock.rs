//! Stand-ins for the MagicBlock programs, registered as LiteSVM builtins.
//!
//! The Magic program is a native builtin of the ER validator (there is no
//! `.so` to load) and the delegation program (DLP) needs its own config and
//! validator key, so both are recording stand-ins: each call's accounts and
//! data are kept (`mocks::take`) and it returns Ok. The SDK's
//! `delegate_account` still does its real part (buffer, zeroed PDA, PDA
//! assigned to the DLP through the System program). Tests assert what our
//! program asks for: the intent's accounts and the delegate args. What the
//! DLP and the committor then do stays covered by the local-stack and devnet
//! end-to-end runs. `DLP_SO` loads a real DLP instead (`with_real_dlp`).

use magicblock_magic_program_api::{
    args::MagicIntentBundleArgs, instruction::MagicBlockInstruction,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::chain::{Chain, Fail, Landed};
use crate::{addr, r, w, DLP, SYSTEM};

/// The discriminator the DLP calls our program with on undelegation.
pub const UNDELEGATE_CALLBACK: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
/// `DelegateConfig::commit_frequency_ms` the program asks for.
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;

pub fn magic_program() -> Address {
    Address::new_from_array(magicblock_magic_program_api::ID.to_bytes())
}

pub fn magic_context() -> Address {
    Address::new_from_array(magicblock_magic_program_api::MAGIC_CONTEXT_PUBKEY.to_bytes())
}

/// One call a stand-in received: (program, account keys, data).
pub type Call = (&'static str, Vec<Address>, Vec<u8>);

pub mod mocks {
    use super::Call;
    use solana_program_runtime::declare_process_instruction;
    use std::cell::RefCell;

    thread_local! {
        /// Every call a stand-in received on this thread (a test is one thread).
        pub static CALLS: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    }

    fn record(
        who: &'static str,
        ic: &solana_program_runtime::invoke_context::InvokeContext,
    ) -> Result<(), solana_instruction::error::InstructionError> {
        let ctx = ic.transaction_context.get_current_instruction_context()?;
        let mut keys = vec![];
        for i in 0..ctx.get_number_of_instruction_accounts() {
            keys.push(*ctx.get_key_of_instruction_account(i)?);
        }
        CALLS.with(|c| {
            c.borrow_mut()
                .push((who, keys, ctx.get_instruction_data().to_vec()))
        });
        Ok(())
    }

    // The Magic program: records the scheduled intent, schedules nothing.
    declare_process_instruction!(MockMagic, 150, |invoke_context| {
        record("magic", invoke_context)
    });
    // The delegation program: records `delegate` (the SDK has already
    // assigned it the PDA).
    declare_process_instruction!(MockDlp, 150, |invoke_context| {
        record("dlp", invoke_context)
    });

    /// The calls recorded since the last `take`.
    pub fn take() -> Vec<Call> {
        CALLS.with(|c| std::mem::take(&mut *c.borrow_mut()))
    }
}

impl Chain {
    /// Registers the Magic and DLP stand-ins and creates the magic context.
    pub fn with_magicblock(mut self) -> Self {
        use solana_program_runtime::solana_sbpf::program::BuiltinFunctionDefinition;
        self.svm
            .add_builtin(magic_program(), mocks::MockMagic::register);
        self.svm.add_builtin(addr(DLP), mocks::MockDlp::register);
        self.put(magic_context(), magic_program(), vec![0; 64]);
        mocks::take();
        self
    }

    /// With `DLP_SO` set, the real delegation program at its id instead of
    /// the stand-in (opt-in, WP14's rollback tests); `None` otherwise.
    pub fn with_real_dlp(mut self) -> Option<Self> {
        let path = std::env::var("DLP_SO").ok()?;
        let so = std::fs::read(&path).unwrap_or_else(|e| panic!("DLP_SO {path}: {e}"));
        self.svm.add_program(addr(DLP), &so).unwrap();
        Some(self)
    }
}

/// The accounts one Magic `ScheduleIntentBundle` call commits, and those it
/// commits and undelegates.
pub fn intent_accounts(call: &Call) -> (Vec<Address>, Vec<Address>) {
    let MagicBlockInstruction::ScheduleIntentBundle(MagicIntentBundleArgs {
        commit,
        commit_and_undelegate,
        ..
    }) = bincode::deserialize(&call.2).expect("a Magic instruction")
    else {
        panic!("not a ScheduleIntentBundle: {:?}", call.2);
    };
    let pick = |ix: &[u8]| ix.iter().map(|i| call.1[*i as usize]).collect::<Vec<_>>();
    (
        commit
            .map(|c| pick(c.committed_accounts_indices()))
            .unwrap_or_default(),
        commit_and_undelegate
            .map(|c| pick(c.committed_accounts_indices()))
            .unwrap_or_default(),
    )
}

/// `DelegateAccountArgs` of one DLP `delegate` call: (commit frequency ms,
/// PDA seeds). The data is the 8-byte discriminator 0, then borsh.
pub fn delegate_args(call: &Call) -> (u32, Vec<Vec<u8>>) {
    assert_eq!(call.2[..8], [0u8; 8], "DLP delegate discriminator");
    let mut rest = &call.2[8..];
    let ms: u32 = borsh::BorshDeserialize::deserialize(&mut rest).unwrap();
    let seeds: Vec<Vec<u8>> = borsh::BorshDeserialize::deserialize(&mut rest).unwrap();
    (ms, seeds)
}

/// The DLP undelegate buffer of `target` (`["undelegate-buffer", pda]`).
pub fn undelegate_buffer(target: &Address) -> Address {
    crate::pda(&addr(DLP), &[b"undelegate-buffer", target.as_ref()])
}

/// The callback the DLP sends on undelegation: discriminator ‖ `seeds`
/// (raw bytes, normally `borsh(Vec<Vec<u8>>)`). `buffer_signs` as the DLP
/// does by CPI.
pub fn callback_ix(
    program: &Address,
    target: &Address,
    payer: &Address,
    seeds: &[u8],
    buffer_signs: bool,
) -> Instruction {
    let mut data = UNDELEGATE_CALLBACK.to_vec();
    data.extend_from_slice(seeds);
    let buffer = undelegate_buffer(target);
    Instruction {
        program_id: *program,
        accounts: vec![
            w(target),
            AccountMeta::new_readonly(buffer, buffer_signs),
            AccountMeta::new(*payer, true),
            r(&addr(SYSTEM)),
        ],
        data,
    }
}

/// As the DLP undelegates `target`: the PDA is gone from the base layer,
/// its content sits in the buffer (owned by `buffer_owner`), and the buffer
/// signs the callback. Needs sigverify off.
pub fn undelegate_callback(
    c: &mut Chain,
    target: &Address,
    seeds: &[Vec<u8>],
    buffer_data: Vec<u8>,
    buffer_owner: Address,
) -> Result<Landed, Fail> {
    let payer = c.funded();
    c.svm
        .set_account(*target, solana_account::Account::default())
        .unwrap();
    c.put(undelegate_buffer(target), buffer_owner, buffer_data);
    let ix = callback_ix(
        &c.program,
        target,
        &solana_signer::Signer::pubkey(&payer),
        &borsh::to_vec(&seeds.to_vec()).unwrap(),
        true,
    );
    c.send_as_with(
        crate::Budget::Light,
        vec![ix],
        &solana_signer::Signer::pubkey(&payer),
    )
}
