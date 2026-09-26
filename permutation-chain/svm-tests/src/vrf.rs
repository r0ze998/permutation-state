//! A stand-in for the MagicBlock VRF program, registered as a LiteSVM
//! builtin at `randomness::VRF_PROGRAM_ID` (WP11 §6.1, WP18 §5).
//!
//! * **Request** (instruction 10 or 11, the scoped requests): checks what
//!   the real program checks of its caller — the program identity
//!   `PDA(["identity"], callback_program)` signs, the queue is writable —
//!   records the request (`requests()`) and logs `VRF_REQ ‖ data`. It
//!   answers nothing by itself.
//! * **Fulfil** (`FULFIL`, sent by a test through `fulfil`): calls the
//!   request's callback program back, as the oracle does, with
//!   `discriminator ‖ E ‖ callback args` and the accounts
//!   `[scoped identity (s), ...callback metas]`, signed as the scoped
//!   identity `PDA(["identity", callback_program], VRF)`.
//!
//! The queues (`randomness::VRF_QUEUE_BASE`, `VRF_QUEUE_ER`) are created at
//! their real addresses, owned by the stand-in.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};

use crate::chain::{Budget, Chain, Fail, Landed};
use crate::r;

/// The stand-in's own instruction: `[FULFIL] ‖ E[32] ‖ borsh(Request)`.
pub const FULFIL: u8 = 0xF0;

/// One callback account of a request.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub pubkey: [u8; 32],
    pub is_signer: bool,
    pub is_writable: bool,
}

/// A randomness request as the VRF program decodes it (after its 8-byte
/// discriminator).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub caller_seed: [u8; 32],
    pub callback_program_id: [u8; 32],
    pub callback_discriminator: Vec<u8>,
    pub callback_accounts_metas: Vec<Meta>,
    pub callback_args: Vec<u8>,
}

/// A request the stand-in received: who paid, on which queue, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Received {
    /// The VRF instruction (10 scoped, 11 scoped high priority).
    pub kind: u8,
    pub payer: Address,
    pub identity: Address,
    pub queue: Address,
    pub request: Request,
}

pub fn vrf_program() -> Address {
    Address::new_from_array(permutation_chain::randomness::VRF_PROGRAM_ID.to_bytes())
}

pub fn queue_base() -> Address {
    Address::new_from_array(permutation_chain::randomness::VRF_QUEUE_BASE.to_bytes())
}

pub fn queue_er() -> Address {
    Address::new_from_array(permutation_chain::randomness::VRF_QUEUE_ER.to_bytes())
}

/// `PDA(["identity"], program)`: the identity that signs `program`'s requests.
pub fn program_identity(program: &Address) -> Address {
    crate::pda(program, &[b"identity"])
}

/// `PDA(["identity", program], VRF)`: the identity that signs callbacks to `program`.
pub fn scoped_identity(program: &Address) -> Address {
    crate::pda(&vrf_program(), &[b"identity", program.as_ref()])
}

pub mod mock {
    use super::{Received, Request, FULFIL};
    use borsh::BorshDeserialize;
    use solana_instruction::error::InstructionError;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_program_runtime::declare_process_instruction;
    use solana_program_runtime::invoke_context::InvokeContext;
    use std::cell::RefCell;

    thread_local! {
        /// Every request the stand-in received on this thread (a test is one thread).
        pub static REQUESTS: RefCell<Vec<Received>> = const { RefCell::new(Vec::new()) };
    }

    fn request(ic: &mut InvokeContext, kind: u8) -> Result<(), InstructionError> {
        let ctx = ic.transaction_context.get_current_instruction_context()?;
        let data = ctx.get_instruction_data().to_vec();
        let req = Request::try_from_slice(
            data.get(8..)
                .ok_or(InstructionError::InvalidInstructionData)?,
        )
        .map_err(|_| InstructionError::InvalidInstructionData)?;
        if ctx.get_number_of_instruction_accounts() < 5 {
            return Err(InstructionError::MissingAccount);
        }
        let payer = *ctx.get_key_of_instruction_account(0)?;
        let identity = *ctx.get_key_of_instruction_account(1)?;
        let queue = *ctx.get_key_of_instruction_account(2)?;
        if !ctx.is_instruction_account_signer(0)? || !ctx.is_instruction_account_signer(1)? {
            return Err(InstructionError::MissingRequiredSignature);
        }
        // The real program ties the identity to the callback program.
        let callback = solana_address::Address::new_from_array(req.callback_program_id);
        if identity != super::program_identity(&callback) {
            return Err(InstructionError::InvalidArgument);
        }
        if !ctx.is_instruction_account_writable(2)? {
            return Err(InstructionError::ReadonlyDataModified);
        }
        solana_program_runtime::stable_log::program_data(
            &ic.get_log_collector(),
            &[b"VRF_REQ", &data],
        );
        REQUESTS.with(|r| {
            r.borrow_mut().push(Received {
                kind,
                payer,
                identity,
                queue,
                request: req,
            })
        });
        Ok(())
    }

    fn fulfil(ic: &mut InvokeContext) -> Result<(), InstructionError> {
        let ctx = ic.transaction_context.get_current_instruction_context()?;
        let data = ctx.get_instruction_data().to_vec();
        let e: [u8; 32] = data
            .get(1..33)
            .and_then(|b| b.try_into().ok())
            .ok_or(InstructionError::InvalidInstructionData)?;
        let req = Request::try_from_slice(&data[33..])
            .map_err(|_| InstructionError::InvalidInstructionData)?;
        let callback = solana_address::Address::new_from_array(req.callback_program_id);
        let (identity, bump) = solana_address::Address::find_program_address(
            &[b"identity", callback.as_ref()],
            &super::vrf_program(),
        );
        let mut accounts = vec![AccountMeta::new_readonly(identity, true)];
        accounts.extend(req.callback_accounts_metas.iter().map(|m| AccountMeta {
            pubkey: solana_address::Address::new_from_array(m.pubkey),
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        }));
        let mut cb_data = req.callback_discriminator.clone();
        cb_data.extend_from_slice(&e);
        cb_data.extend_from_slice(&req.callback_args);
        ic.native_invoke_signed(
            Instruction {
                program_id: callback,
                accounts,
                data: cb_data,
            },
            &[&[b"identity", callback.as_ref(), &[bump]]],
        )
    }

    declare_process_instruction!(MockVrf, 150, |invoke_context| {
        let kind = invoke_context
            .transaction_context
            .get_current_instruction_context()?
            .get_instruction_data()
            .first()
            .copied();
        match kind {
            Some(k @ (10 | 11)) => request(invoke_context, k),
            Some(FULFIL) => fulfil(invoke_context),
            _ => Err(InstructionError::InvalidInstructionData),
        }
    });

    /// The requests received since the last `take`.
    pub fn take() -> Vec<Received> {
        REQUESTS.with(|r| std::mem::take(&mut *r.borrow_mut()))
    }
}

/// The requests the stand-in received since the last call.
pub fn requests() -> Vec<Received> {
    mock::take()
}

impl Chain {
    /// Registers the VRF stand-in and creates both oracle queues.
    pub fn with_vrf(mut self) -> Self {
        use solana_program_runtime::solana_sbpf::program::BuiltinFunctionDefinition;
        self.svm.add_builtin(vrf_program(), mock::MockVrf::register);
        self.put(queue_base(), vrf_program(), vec![0; 1024]);
        self.put(queue_er(), vrf_program(), vec![0; 1024]);
        mock::take();
        self
    }
}

/// The oracle's answer to `request` with output `e`: the stand-in calls the
/// request's callback program back, signed as its scoped identity. The
/// callback program must be in the transaction, and so must its accounts.
pub fn fulfil_ix(request: &Request, e: [u8; 32]) -> Instruction {
    let callback = Address::new_from_array(request.callback_program_id);
    let mut data = vec![FULFIL];
    data.extend_from_slice(&e);
    data.extend_from_slice(&borsh::to_vec(request).unwrap());
    let mut accounts = vec![r(&scoped_identity(&callback)), r(&callback)];
    accounts.extend(request.callback_accounts_metas.iter().map(|m| AccountMeta {
        pubkey: Address::new_from_array(m.pubkey),
        is_signer: false,
        is_writable: m.is_writable,
    }));
    Instruction {
        program_id: vrf_program(),
        accounts,
        data,
    }
}

/// Sends `fulfil_ix(request, e)` (any fee payer: the oracle's).
pub fn fulfil(c: &mut Chain, request: &Request, e: [u8; 32]) -> Result<Landed, Fail> {
    let oracle = c.funded();
    c.send_with(Budget::Light, vec![fulfil_ix(request, e)], &[&oracle])
}

#[cfg(test)]
mod tests {
    use super::*;
    use permutation_chain::randomness;
    use solana_program::pubkey::Pubkey;

    fn pk(a: &Address) -> Pubkey {
        Pubkey::new_from_array(a.to_bytes())
    }

    /// The program's request instruction decodes into the stand-in's layout.
    #[test]
    fn requests_decode_from_the_programs_layout() {
        let program = Address::new_unique();
        let (payer, chunk0) = (Address::new_unique(), Address::new_unique());
        let ix = randomness::request_ix(
            &pk(&program),
            &pk(&payer),
            &randomness::VRF_QUEUE_ER,
            [3; 32],
            randomness::CONSUME_TICK_TAG,
            &pk(&chunk0),
            &randomness::tick_callback_args(42, 7),
        );
        assert_eq!(
            Address::new_from_array(ix.program_id.to_bytes()),
            vrf_program()
        );
        assert_eq!(
            Address::new_from_array(ix.accounts[1].pubkey.to_bytes()),
            program_identity(&program)
        );
        let req = Request::try_from_slice(&ix.data[8..]).unwrap();
        assert_eq!(req.callback_program_id, program.to_bytes());
        assert_eq!(
            req.callback_discriminator,
            vec![randomness::CONSUME_TICK_TAG]
        );
        assert_eq!(
            req.callback_accounts_metas,
            vec![Meta {
                pubkey: chunk0.to_bytes(),
                is_signer: false,
                is_writable: true
            }]
        );
        assert_eq!(req.callback_args[..8], 42u64.to_le_bytes());
        // The fulfilment calls back with `tag ‖ E ‖ args`, the scoped
        // identity first.
        let f = fulfil_ix(&req, [9; 32]);
        assert_eq!(f.accounts[0].pubkey, scoped_identity(&program));
        assert_eq!(
            pk(&scoped_identity(&program)),
            randomness::scoped_vrf_identity(&pk(&program))
        );
        assert_eq!(f.data[0], FULFIL);
    }

    /// A builtin stand-in at the VRF's address: a callback program that is
    /// not ours (the system program, which refuses the data) makes the
    /// fulfilment fail; one that is not in the transaction too. The full
    /// round trip through the program lands with FreezeTick (unit P1).
    #[test]
    fn the_stand_in_answers_only_its_own_instructions() {
        let mut c = Chain::new().with_vrf();
        assert_eq!(c.owner(&queue_er()), Some(vrf_program()));
        assert_eq!(c.owner(&queue_base()), Some(vrf_program()));
        let k = c.funded();
        let junk = Instruction {
            program_id: vrf_program(),
            accounts: vec![],
            data: vec![0x42],
        };
        assert!(c.send(vec![junk], &[&k]).is_err());
        // A request that the identity did not sign is refused.
        let program = c.program;
        let ix = randomness::request_ix(
            &pk(&program),
            &pk(&solana_signer::Signer::pubkey(&k)),
            &randomness::VRF_QUEUE_ER,
            [3; 32],
            randomness::CONSUME_TICK_TAG,
            &Pubkey::new_unique(),
            &[],
        );
        let mut ix = Instruction {
            program_id: vrf_program(),
            accounts: ix
                .accounts
                .iter()
                .map(|m| AccountMeta {
                    pubkey: Address::new_from_array(m.pubkey.to_bytes()),
                    is_signer: m.is_signer,
                    is_writable: m.is_writable,
                })
                .collect(),
            data: ix.data,
        };
        ix.accounts[1].is_signer = false;
        assert!(c.send(vec![ix], &[&k]).is_err());
        assert!(requests().is_empty());
    }

    /// The fulfilment reaches the callback program by CPI, signed as the
    /// scoped identity, with `tag ‖ E ‖ args`: our program is called
    /// (depth 2) with `ConsumeTickRandomness`, which refuses it until unit
    /// P1 implements it.
    #[test]
    fn fulfil_calls_the_program_back() {
        let mut c = Chain::new().with_vrf();
        let chunk0 = Address::new_unique();
        c.put(chunk0, c.program, vec![0; 64]);
        let request = Request {
            caller_seed: [3; 32],
            callback_program_id: c.program.to_bytes(),
            callback_discriminator: vec![randomness::CONSUME_TICK_TAG],
            callback_accounts_metas: vec![Meta {
                pubkey: chunk0.to_bytes(),
                is_signer: false,
                is_writable: true,
            }],
            callback_args: randomness::tick_callback_args(7, 0).to_vec(),
        };
        let fail = fulfil(&mut c, &request, [9; 32]).unwrap_err();
        assert_eq!(fail.code, Some(1), "{fail:?}");
        let ours = format!("Program {} invoke [2]", c.program);
        assert!(fail.logs.iter().any(|l| l == &ours), "{:?}", fail.logs);
    }
}
