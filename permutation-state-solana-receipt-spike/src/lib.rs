#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

pub const VERSION: u8 = 1;
pub const WORKSITE_SEED: &[u8] = b"worksite";
pub const ACCOUNT_SPACE: usize = 384;
pub const STATE_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_STATE/V1";
pub const EVENT_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_EVENT/V1";
pub const GENESIS_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_GENESIS/V1";
pub const LOG_DOMAIN: &[u8] = b"PERMSTATE_EVENT_V1";
pub const UNDELEGATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;

pub const EVENT_MARA_CHOICE: u8 = 0;
pub const EVENT_IVO_CHOICE: u8 = 1;
pub const EVENT_MANDATE_ACCEPTED: u8 = 2;

pub const ACTION_OATH: u16 = 1;
pub const ACTION_CHARTER: u16 = 2;
pub const ACTION_SERVICE: u16 = 11;
pub const ACTION_MILLRACE: u16 = 12;
pub const ACTION_CUT: u16 = 13;
pub const ACTION_RECONCILE: u16 = 14;

pub const MANDATE_M_032: u16 = 1_032;
pub const MANDATE_M_033: u16 = 1_033;
pub const MANDATE_E_019: u16 = 2_019;
pub const MANDATE_E_020: u16 = 2_020;
pub const MANDATE_E_021: u16 = 2_021;
pub const MANDATE_E_022: u16 = 2_022;
pub const MANDATE_S_044: u16 = 3_044;
pub const MANDATE_S_045: u16 = 3_045;
pub const MANDATE_S_046: u16 = 3_046;
pub const MANDATE_S_047: u16 = 3_047;
pub const MANDATE_W_014: u16 = 4_014;

const STAGE_MARA: u8 = 0;
const STAGE_IVO: u8 = 1;
const STAGE_SUCCESSOR: u8 = 2;
const STAGE_CONTINUING: u8 = 3;
const SEASON_ACTIVE: u8 = 1;
const SETTLEMENT_NOT_STARTED: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InitializeArgs {
    pub season_id: [u8; 32],
    pub worksite_id: [u8; 32],
    pub ruleset_hash: [u8; 32],
    pub envoy: Pubkey,
    pub maker: Pubkey,
    pub successor: Pubkey,
    pub expected_genesis_state_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApplyEventArgs {
    pub event_kind: u8,
    pub action: u16,
    pub expected_seq: u64,
    pub ruleset_hash: [u8; 32],
    pub prior_state_root: [u8; 32],
    pub expected_new_state_root: [u8; 32],
    pub expected_prev_event_hash: [u8; 32],
    pub client_event_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PermutationInstruction {
    Initialize(InitializeArgs),
    ApplyWorksiteEvent(ApplyEventArgs),
    Delegate,
    Commit,
    CommitAndUndelegate,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WorksiteState {
    pub version: u8,
    pub bump: u8,
    pub authority: Pubkey,
    pub envoy: Pubkey,
    pub maker: Pubkey,
    pub successor: Pubkey,
    pub season_id: [u8; 32],
    pub worksite_id: [u8; 32],
    pub ruleset_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub head_event_hash: [u8; 32],
    pub seq: u64,
    pub stage: u8,
    pub season_status: u8,
    pub settlement_status: u8,
    pub claim_available: bool,
    pub branch: u8,
    pub resolution: u8,
    pub water: i16,
    pub food: i16,
    pub cohesion: i16,
    pub timber: i16,
    pub prosperity: i16,
    pub food_debt: i16,
    pub tala_trust: i16,
    pub service_stair: u8,
    pub riverkeepers: u8,
    pub memory_receipt: u8,
    pub worksite_status: u8,
    pub mandate_ids: [u16; 3],
    pub mandate_status: [u8; 3],
    pub accepted_mandate: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
struct StateRootView {
    authority: Pubkey,
    envoy: Pubkey,
    maker: Pubkey,
    successor: Pubkey,
    season_id: [u8; 32],
    worksite_id: [u8; 32],
    ruleset_hash: [u8; 32],
    seq: u64,
    stage: u8,
    season_status: u8,
    settlement_status: u8,
    claim_available: bool,
    branch: u8,
    resolution: u8,
    water: i16,
    food: i16,
    cohesion: i16,
    timber: i16,
    prosperity: i16,
    food_debt: i16,
    tala_trust: i16,
    service_stair: u8,
    riverkeepers: u8,
    memory_receipt: u8,
    worksite_status: u8,
    mandate_ids: [u16; 3],
    mandate_status: [u8; 3],
    accepted_mandate: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReceiptV1 {
    pub version: u8,
    pub seq: u64,
    pub event_kind: u8,
    pub action: u16,
    pub actor: Pubkey,
    pub ruleset_hash: [u8; 32],
    pub prior_state_root: [u8; 32],
    pub new_state_root: [u8; 32],
    pub previous_event_hash: [u8; 32],
    pub event_hash: [u8; 32],
    pub client_event_hash: [u8; 32],
    pub season_active: bool,
    pub settlement_not_started: bool,
    pub claim_unavailable: bool,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptError {
    InvalidInstruction = 1,
    MissingSignature = 2,
    WrongPda = 3,
    WrongOwner = 4,
    AlreadyInitialized = 5,
    StateEncoding = 6,
    UnauthorizedActor = 7,
    StaleSequence = 8,
    RulesetMismatch = 9,
    PriorRootMismatch = 10,
    PreviousEventMismatch = 11,
    IllegalStage = 12,
    IllegalBranchAction = 13,
    NewRootMismatch = 14,
    InvariantViolation = 15,
    MandateNotQueued = 16,
    UnauthorizedLifecycleActor = 17,
    WrongDelegationProgram = 18,
    WrongMagicProgram = 19,
    WrongMagicContext = 20,
    InvalidUndelegateSeeds = 21,
    StoredRootMismatch = 22,
}

impl From<ReceiptError> for ProgramError {
    fn from(value: ReceiptError) -> Self {
        ProgramError::Custom(value as u32)
    }
}

impl WorksiteState {
    pub fn new(
        program_id: &Pubkey,
        state_pda: &Pubkey,
        bump: u8,
        authority: Pubkey,
        args: &InitializeArgs,
    ) -> Result<Self, ProgramError> {
        let mut state = Self {
            version: VERSION,
            bump,
            authority,
            envoy: args.envoy,
            maker: args.maker,
            successor: args.successor,
            season_id: args.season_id,
            worksite_id: args.worksite_id,
            ruleset_hash: args.ruleset_hash,
            state_root: [0; 32],
            head_event_hash: [0; 32],
            seq: 0,
            stage: STAGE_MARA,
            season_status: SEASON_ACTIVE,
            settlement_status: SETTLEMENT_NOT_STARTED,
            claim_available: false,
            branch: 0,
            resolution: 0,
            water: 28,
            food: 46,
            cohesion: 54,
            timber: 18,
            prosperity: 38,
            food_debt: 0,
            tala_trust: 0,
            service_stair: 0,
            riverkeepers: 0,
            memory_receipt: 0,
            worksite_status: 0,
            mandate_ids: [0; 3],
            mandate_status: [0; 3],
            accepted_mandate: 0,
        };
        state.state_root = state.compute_state_root()?;
        state.head_event_hash = hashv(&[
            GENESIS_DOMAIN,
            program_id.as_ref(),
            state_pda.as_ref(),
            &state.ruleset_hash,
            &state.state_root,
        ])
        .to_bytes();
        Ok(state)
    }

    fn root_view(&self) -> StateRootView {
        StateRootView {
            authority: self.authority,
            envoy: self.envoy,
            maker: self.maker,
            successor: self.successor,
            season_id: self.season_id,
            worksite_id: self.worksite_id,
            ruleset_hash: self.ruleset_hash,
            seq: self.seq,
            stage: self.stage,
            season_status: self.season_status,
            settlement_status: self.settlement_status,
            claim_available: self.claim_available,
            branch: self.branch,
            resolution: self.resolution,
            water: self.water,
            food: self.food,
            cohesion: self.cohesion,
            timber: self.timber,
            prosperity: self.prosperity,
            food_debt: self.food_debt,
            tala_trust: self.tala_trust,
            service_stair: self.service_stair,
            riverkeepers: self.riverkeepers,
            memory_receipt: self.memory_receipt,
            worksite_status: self.worksite_status,
            mandate_ids: self.mandate_ids,
            mandate_status: self.mandate_status,
            accepted_mandate: self.accepted_mandate,
        }
    }

    pub fn compute_state_root(&self) -> Result<[u8; 32], ProgramError> {
        let bytes = borsh::to_vec(&self.root_view()).map_err(|_| ReceiptError::StateEncoding)?;
        Ok(hashv(&[STATE_DOMAIN, &bytes]).to_bytes())
    }

    pub fn assert_invariants(&self) -> ProgramResult {
        if self.season_status != SEASON_ACTIVE
            || self.settlement_status != SETTLEMENT_NOT_STARTED
            || self.claim_available
        {
            return Err(ReceiptError::InvariantViolation.into());
        }
        Ok(())
    }

    fn expected_actor(&self) -> Result<Pubkey, ProgramError> {
        match self.stage {
            STAGE_MARA => Ok(self.envoy),
            STAGE_IVO => Ok(self.maker),
            STAGE_SUCCESSOR => Ok(self.successor),
            _ => Err(ReceiptError::IllegalStage.into()),
        }
    }

    fn reduce(&mut self, event_kind: u8, action: u16) -> ProgramResult {
        match (self.stage, event_kind) {
            (STAGE_MARA, EVENT_MARA_CHOICE) => match action {
                ACTION_OATH => {
                    self.branch = 1;
                    self.water = 36;
                    self.cohesion = 60;
                    self.food_debt = 12;
                    self.tala_trust = 20;
                    self.service_stair = 1;
                    self.riverkeepers = 1;
                    self.memory_receipt = 1;
                }
                ACTION_CHARTER => {
                    self.branch = 2;
                    self.water = 33;
                    self.cohesion = 46;
                    self.food_debt = 0;
                    self.tala_trust = -20;
                    self.service_stair = 2;
                    self.riverkeepers = 2;
                    self.memory_receipt = 2;
                }
                _ => return Err(ReceiptError::IllegalBranchAction.into()),
            },
            (STAGE_IVO, EVENT_IVO_CHOICE) => {
                match (self.branch, action) {
                    (1, ACTION_SERVICE) => {
                        self.resolution = 1;
                        self.water += 24;
                        self.cohesion += 4;
                        self.timber -= 6;
                        self.prosperity += 5;
                        self.tala_trust = 28;
                        self.mandate_ids = [MANDATE_M_032, MANDATE_E_019, MANDATE_S_044];
                    }
                    (1, ACTION_MILLRACE) => {
                        self.resolution = 2;
                        self.water += 18;
                        self.cohesion -= 4;
                        self.timber -= 3;
                        self.prosperity += 3;
                        self.tala_trust = 8;
                        self.mandate_ids = [MANDATE_E_020, MANDATE_M_032, MANDATE_S_045];
                    }
                    (2, ACTION_CUT) => {
                        self.resolution = 3;
                        self.water += 22;
                        self.cohesion -= 5;
                        self.timber -= 10;
                        self.prosperity += 1;
                        self.tala_trust = -35;
                        self.mandate_ids = [MANDATE_E_021, MANDATE_W_014, MANDATE_S_046];
                    }
                    (2, ACTION_RECONCILE) => {
                        self.resolution = 4;
                        self.water += 18;
                        self.cohesion += 8;
                        self.timber -= 6;
                        self.prosperity += 4;
                        self.tala_trust = 5;
                        self.food_debt = 8;
                        self.service_stair = 1;
                        self.riverkeepers = 3;
                        self.mandate_ids = [MANDATE_M_033, MANDATE_E_022, MANDATE_S_047];
                    }
                    _ => return Err(ReceiptError::IllegalBranchAction.into()),
                }
                self.mandate_status = [0; 3];
            }
            (STAGE_SUCCESSOR, EVENT_MANDATE_ACCEPTED) => {
                let Some(index) = self.mandate_ids.iter().position(|id| *id == action) else {
                    return Err(ReceiptError::MandateNotQueued.into());
                };
                if self.mandate_status[index] != 0 || self.accepted_mandate != 0 {
                    return Err(ReceiptError::MandateNotQueued.into());
                }
                self.mandate_status[index] = 1;
                self.accepted_mandate = action;
            }
            _ => return Err(ReceiptError::IllegalStage.into()),
        }

        self.stage = match event_kind {
            EVENT_MARA_CHOICE => STAGE_IVO,
            EVENT_IVO_CHOICE => STAGE_SUCCESSOR,
            EVENT_MANDATE_ACCEPTED => STAGE_CONTINUING,
            _ => return Err(ReceiptError::InvalidInstruction.into()),
        };
        self.worksite_status = match event_kind {
            EVENT_MARA_CHOICE => 1,
            EVENT_IVO_CHOICE if action == ACTION_CUT => 3,
            EVENT_IVO_CHOICE => 2,
            EVENT_MANDATE_ACCEPTED => self.worksite_status,
            _ => return Err(ReceiptError::InvalidInstruction.into()),
        };
        self.seq = self
            .seq
            .checked_add(1)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn preview_event(
        &self,
        actor: &Pubkey,
        args: &ApplyEventArgs,
    ) -> Result<(Self, ReceiptV1), ProgramError> {
        self.assert_invariants()?;
        if &self.expected_actor()? != actor {
            return Err(ReceiptError::UnauthorizedActor.into());
        }
        if args.expected_seq != self.seq {
            return Err(ReceiptError::StaleSequence.into());
        }
        if args.ruleset_hash != self.ruleset_hash {
            return Err(ReceiptError::RulesetMismatch.into());
        }
        if args.prior_state_root != self.state_root {
            return Err(ReceiptError::PriorRootMismatch.into());
        }
        if args.expected_prev_event_hash != self.head_event_hash {
            return Err(ReceiptError::PreviousEventMismatch.into());
        }

        let mut next = self.clone();
        next.reduce(args.event_kind, args.action)?;
        next.assert_invariants()?;
        let new_state_root = next.compute_state_root()?;
        if args.expected_new_state_root != new_state_root {
            return Err(ReceiptError::NewRootMismatch.into());
        }

        let seq_bytes = args.expected_seq.to_le_bytes();
        let action_bytes = args.action.to_le_bytes();
        let event_kind = [args.event_kind];
        let event_hash = hashv(&[
            EVENT_DOMAIN,
            &args.ruleset_hash,
            &args.expected_prev_event_hash,
            &args.prior_state_root,
            &new_state_root,
            actor.as_ref(),
            &seq_bytes,
            &event_kind,
            &action_bytes,
            &args.client_event_hash,
        ])
        .to_bytes();

        next.state_root = new_state_root;
        next.head_event_hash = event_hash;
        let receipt = ReceiptV1 {
            version: VERSION,
            seq: args.expected_seq,
            event_kind: args.event_kind,
            action: args.action,
            actor: *actor,
            ruleset_hash: args.ruleset_hash,
            prior_state_root: args.prior_state_root,
            new_state_root,
            previous_event_hash: args.expected_prev_event_hash,
            event_hash,
            client_event_hash: args.client_event_hash,
            season_active: true,
            settlement_not_started: true,
            claim_unavailable: true,
        };
        Ok((next, receipt))
    }
}

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if let Some(seed_bytes) = instruction_data.strip_prefix(&UNDELEGATE_CALLBACK_DISCRIMINATOR) {
        let pda_seeds = Vec::<Vec<u8>>::try_from_slice(seed_bytes)
            .map_err(|_| ReceiptError::InvalidInstruction)?;
        return process_undelegate_callback(program_id, accounts, pda_seeds);
    }

    let instruction = PermutationInstruction::try_from_slice(instruction_data)
        .map_err(|_| ReceiptError::InvalidInstruction)?;
    match instruction {
        PermutationInstruction::Initialize(args) => process_initialize(program_id, accounts, args),
        PermutationInstruction::ApplyWorksiteEvent(args) => {
            process_apply(program_id, accounts, args)
        }
        PermutationInstruction::Delegate => process_delegate(program_id, accounts),
        PermutationInstruction::Commit => process_commit(program_id, accounts),
        PermutationInstruction::CommitAndUndelegate => {
            process_commit_and_undelegate(program_id, accounts)
        }
    }
}

fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: InitializeArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let state_account = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;

    if !authority.is_signer {
        return Err(ReceiptError::MissingSignature.into());
    }
    if !authority.is_writable || !state_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[WORKSITE_SEED, &args.season_id, &args.worksite_id],
        program_id,
    );
    if state_account.key != &expected_pda {
        return Err(ReceiptError::WrongPda.into());
    }
    if state_account.owner == program_id || !state_account.data_is_empty() {
        return Err(ReceiptError::AlreadyInitialized.into());
    }

    let lamports = Rent::get()?.minimum_balance(ACCOUNT_SPACE);
    let bump_seed = [bump];
    let signer_seeds: &[&[u8]] = &[
        WORKSITE_SEED,
        &args.season_id,
        &args.worksite_id,
        &bump_seed,
    ];
    invoke_signed(
        &system_instruction::create_account(
            authority.key,
            state_account.key,
            lamports,
            ACCOUNT_SPACE as u64,
            program_id,
        ),
        &[authority.clone(), state_account.clone(), system.clone()],
        &[signer_seeds],
    )?;

    let state = WorksiteState::new(program_id, state_account.key, bump, *authority.key, &args)?;
    if state.state_root != args.expected_genesis_state_root {
        return Err(ReceiptError::NewRootMismatch.into());
    }
    write_state(state_account, &state)?;
    msg!("PERMSTATE initialized seq=0");
    Ok(())
}

/// Base-layer instruction. Delegates the canonical Worksite PDA to the
/// validator selected by the client (or MagicBlock's default when omitted).
fn process_delegate(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;
    let state_account = next_account_info(&mut accounts)?;
    let owner_program = next_account_info(&mut accounts)?;
    let delegation_buffer = next_account_info(&mut accounts)?;
    let delegation_record = next_account_info(&mut accounts)?;
    let delegation_metadata = next_account_info(&mut accounts)?;
    let delegation_program = next_account_info(&mut accounts)?;
    let validator = accounts.next();

    require_signer(authority)?;
    if !authority.is_writable || !state_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if owner_program.key != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    if delegation_program.key != &DELEGATION_PROGRAM_ID {
        return Err(ReceiptError::WrongDelegationProgram.into());
    }

    let state = read_owned_worksite_state(program_id, state_account)?;
    if authority.key != &state.authority {
        return Err(ReceiptError::UnauthorizedLifecycleActor.into());
    }

    let pda_seeds: &[&[u8]] = &[WORKSITE_SEED, &state.season_id, &state.worksite_id];
    let delegate_accounts = DelegateAccounts {
        payer: authority,
        pda: state_account,
        owner_program,
        buffer: delegation_buffer,
        delegation_record,
        delegation_metadata,
        delegation_program,
        system_program: system,
    };
    let delegate_config = DelegateConfig {
        commit_frequency_ms: ER_COMMIT_FREQUENCY_MS,
        validator: validator.map(|account| *account.key),
    };

    delegate_account(delegate_accounts, pda_seeds, delegate_config)?;
    msg!(
        "PERMSTATE delegated worksite={} commit_frequency_ms={}",
        state_account.key,
        ER_COMMIT_FREQUENCY_MS
    );
    Ok(())
}

/// ER instruction. Any assigned participant may checkpoint the current
/// Worksite state; checkpointing does not alter game state or end the session.
fn process_commit(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let (payer, state_account, magic_program, magic_context, state) =
        lifecycle_accounts(program_id, accounts)?;
    if !is_worksite_participant(&state, payer.key) {
        return Err(ReceiptError::UnauthorizedLifecycleActor.into());
    }

    MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone())
        .commit(std::slice::from_ref(state_account))
        .build_and_invoke()?;

    msg!(
        "PERMSTATE scheduled ER commit worksite={} seq={}",
        state_account.key,
        state.seq
    );
    Ok(())
}

/// ER instruction. Ending delegation is authority-only because it removes the
/// Worksite from the low-latency session for every active participant.
fn process_commit_and_undelegate(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let (authority, state_account, magic_program, magic_context, state) =
        lifecycle_accounts(program_id, accounts)?;
    if authority.key != &state.authority {
        return Err(ReceiptError::UnauthorizedLifecycleActor.into());
    }

    MagicIntentBundleBuilder::new(
        authority.clone(),
        magic_context.clone(),
        magic_program.clone(),
    )
    .commit_and_undelegate(std::slice::from_ref(state_account))
    .build_and_invoke()?;

    msg!(
        "PERMSTATE scheduled ER commit+undelegate worksite={} seq={}",
        state_account.key,
        state.seq
    );
    Ok(())
}

/// Base-layer CPI callback. The delegation program invokes this exact
/// discriminator after the ER finalizes undelegation. It recreates the
/// canonical Worksite PDA and copies the finalized buffer into it.
fn process_undelegate_callback(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    pda_seeds: Vec<Vec<u8>>,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let delegated_pda = next_account_info(&mut accounts)?;
    let delegation_buffer = next_account_info(&mut accounts)?;
    let payer = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;

    if !delegated_pda.is_writable || !delegation_buffer.is_writable || !payer.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if delegation_buffer.owner != &DELEGATION_PROGRAM_ID {
        return Err(ReceiptError::WrongDelegationProgram.into());
    }

    let finalized_state = read_state(delegation_buffer)?;
    validate_worksite_identity(program_id, delegated_pda.key, &finalized_state)?;
    validate_stored_state(&finalized_state)?;
    validate_undelegate_seeds(&finalized_state, &pda_seeds)?;

    undelegate_account(
        delegated_pda,
        program_id,
        delegation_buffer,
        payer,
        system,
        pda_seeds,
    )?;
    msg!(
        "PERMSTATE undelegated worksite={} seq={}",
        delegated_pda.key,
        finalized_state.seq
    );
    Ok(())
}

fn process_apply(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: ApplyEventArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let actor = next_account_info(&mut accounts)?;
    let state_account = next_account_info(&mut accounts)?;

    if !actor.is_signer {
        return Err(ReceiptError::MissingSignature.into());
    }
    if !state_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    let state = read_owned_worksite_state(program_id, state_account)?;

    let (next, receipt) = state.preview_event(actor.key, &args)?;
    write_state(state_account, &next)?;
    let receipt_bytes = borsh::to_vec(&receipt).map_err(|_| ReceiptError::StateEncoding)?;
    solana_program::log::sol_log_data(&[LOG_DOMAIN, &receipt_bytes]);
    msg!(
        "PERMSTATE accepted seq={} kind={} action={}",
        receipt.seq,
        receipt.event_kind,
        receipt.action
    );
    Ok(())
}

fn lifecycle_accounts<'a, 'info>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'info>],
) -> Result<
    (
        &'a AccountInfo<'info>,
        &'a AccountInfo<'info>,
        &'a AccountInfo<'info>,
        &'a AccountInfo<'info>,
        WorksiteState,
    ),
    ProgramError,
> {
    let mut accounts = accounts.iter();
    let payer = next_account_info(&mut accounts)?;
    let state_account = next_account_info(&mut accounts)?;
    let magic_program = next_account_info(&mut accounts)?;
    let magic_context = next_account_info(&mut accounts)?;

    require_signer(payer)?;
    if !payer.is_writable || !state_account.is_writable || !magic_context.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if magic_program.key != &MAGIC_PROGRAM_ID {
        return Err(ReceiptError::WrongMagicProgram.into());
    }
    if magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ReceiptError::WrongMagicContext.into());
    }
    let state = read_owned_worksite_state(program_id, state_account)?;
    Ok((payer, state_account, magic_program, magic_context, state))
}

fn require_signer(account: &AccountInfo) -> ProgramResult {
    if !account.is_signer {
        return Err(ReceiptError::MissingSignature.into());
    }
    Ok(())
}

fn is_worksite_participant(state: &WorksiteState, key: &Pubkey) -> bool {
    key == &state.authority || key == &state.envoy || key == &state.maker || key == &state.successor
}

fn read_owned_worksite_state(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<WorksiteState, ProgramError> {
    if account.owner != program_id {
        return Err(ReceiptError::WrongOwner.into());
    }
    let state = read_state(account)?;
    validate_worksite_identity(program_id, account.key, &state)?;
    validate_stored_state(&state)?;
    Ok(state)
}

fn validate_worksite_identity(
    program_id: &Pubkey,
    state_key: &Pubkey,
    state: &WorksiteState,
) -> ProgramResult {
    if state.version != VERSION {
        return Err(ReceiptError::StateEncoding.into());
    }
    let (expected_pda, expected_bump) = Pubkey::find_program_address(
        &[WORKSITE_SEED, &state.season_id, &state.worksite_id],
        program_id,
    );
    if state_key != &expected_pda || state.bump != expected_bump {
        return Err(ReceiptError::WrongPda.into());
    }
    Ok(())
}

fn validate_stored_state(state: &WorksiteState) -> ProgramResult {
    state.assert_invariants()?;
    if state.compute_state_root()? != state.state_root {
        return Err(ReceiptError::StoredRootMismatch.into());
    }
    Ok(())
}

fn validate_undelegate_seeds(state: &WorksiteState, pda_seeds: &[Vec<u8>]) -> ProgramResult {
    let expected = [
        WORKSITE_SEED.to_vec(),
        state.season_id.to_vec(),
        state.worksite_id.to_vec(),
    ];
    if pda_seeds != expected {
        return Err(ReceiptError::InvalidUndelegateSeeds.into());
    }
    Ok(())
}

fn read_state(account: &AccountInfo) -> Result<WorksiteState, ProgramError> {
    let data = account.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    WorksiteState::deserialize(&mut slice).map_err(|_| ReceiptError::StateEncoding.into())
}

fn write_state(account: &AccountInfo, state: &WorksiteState) -> ProgramResult {
    let bytes = borsh::to_vec(state).map_err(|_| ReceiptError::StateEncoding)?;
    if bytes.len() > account.data_len() {
        return Err(ProgramError::AccountDataTooSmall);
    }
    let mut data = account.try_borrow_mut_data()?;
    data.fill(0);
    data[..bytes.len()].copy_from_slice(&bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes32(value: u8) -> [u8; 32] {
        [value; 32]
    }

    fn fixture() -> (Pubkey, Pubkey, WorksiteState, Pubkey, Pubkey, Pubkey) {
        let program_id = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let envoy = Pubkey::new_unique();
        let maker = Pubkey::new_unique();
        let successor = Pubkey::new_unique();
        let season_id = bytes32(7);
        let worksite_id = bytes32(31);
        let (state_pda, bump) =
            Pubkey::find_program_address(&[WORKSITE_SEED, &season_id, &worksite_id], &program_id);
        let args = InitializeArgs {
            season_id,
            worksite_id,
            ruleset_hash: bytes32(9),
            envoy,
            maker,
            successor,
            expected_genesis_state_root: [0; 32],
        };
        let state = WorksiteState::new(&program_id, &state_pda, bump, authority, &args).unwrap();
        (program_id, state_pda, state, envoy, maker, successor)
    }

    fn args_for(
        state: &WorksiteState,
        actor: &Pubkey,
        event_kind: u8,
        action: u16,
    ) -> ApplyEventArgs {
        let mut args = ApplyEventArgs {
            event_kind,
            action,
            expected_seq: state.seq,
            ruleset_hash: state.ruleset_hash,
            prior_state_root: state.state_root,
            expected_new_state_root: [0; 32],
            expected_prev_event_hash: state.head_event_hash,
            client_event_hash: bytes32((state.seq + 40) as u8),
        };
        let mut preview = state.clone();
        assert_eq!(preview.expected_actor().unwrap(), *actor);
        preview.reduce(event_kind, action).unwrap();
        args.expected_new_state_root = preview.compute_state_root().unwrap();
        args
    }

    fn apply(state: &WorksiteState, actor: &Pubkey, event_kind: u8, action: u16) -> WorksiteState {
        let args = args_for(state, actor, event_kind, action);
        state.preview_event(actor, &args).unwrap().0
    }

    #[test]
    fn oath_service_mandate_preserves_season_and_claim_invariants() {
        let (_, _, genesis, envoy, maker, successor) = fixture();
        let after_mara = apply(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);
        assert_eq!(after_mara.stage, STAGE_IVO);
        assert_eq!(after_mara.food_debt, 12);
        assert_eq!(after_mara.service_stair, 1);

        let after_ivo = apply(&after_mara, &maker, EVENT_IVO_CHOICE, ACTION_SERVICE);
        assert_eq!(after_ivo.stage, STAGE_SUCCESSOR);
        assert_eq!(after_ivo.resolution, 1);
        assert_eq!(after_ivo.water, 60);
        assert_eq!(
            after_ivo.mandate_ids,
            [MANDATE_M_032, MANDATE_E_019, MANDATE_S_044]
        );

        let final_state = apply(
            &after_ivo,
            &successor,
            EVENT_MANDATE_ACCEPTED,
            MANDATE_M_032,
        );
        assert_eq!(final_state.stage, STAGE_CONTINUING);
        assert_eq!(final_state.accepted_mandate, MANDATE_M_032);
        assert_eq!(final_state.season_status, SEASON_ACTIVE);
        assert_eq!(final_state.settlement_status, SETTLEMENT_NOT_STARTED);
        assert!(!final_state.claim_available);
        assert_eq!(final_state.seq, 3);
        final_state.assert_invariants().unwrap();
    }

    #[test]
    fn all_four_resolutions_are_branch_gated() {
        let (_, _, genesis, envoy, maker, _) = fixture();
        let oath = apply(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);
        for (action, resolution) in [(ACTION_SERVICE, 1), (ACTION_MILLRACE, 2)] {
            assert_eq!(
                apply(&oath, &maker, EVENT_IVO_CHOICE, action).resolution,
                resolution
            );
        }
        let wrong = args_for(&oath, &maker, EVENT_IVO_CHOICE, ACTION_SERVICE);
        let mut illegal = wrong.clone();
        illegal.action = ACTION_CUT;
        assert_eq!(
            oath.preview_event(&maker, &illegal).unwrap_err(),
            ProgramError::Custom(ReceiptError::IllegalBranchAction as u32)
        );

        let charter = apply(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_CHARTER);
        for (action, resolution) in [(ACTION_CUT, 3), (ACTION_RECONCILE, 4)] {
            assert_eq!(
                apply(&charter, &maker, EVENT_IVO_CHOICE, action).resolution,
                resolution
            );
        }
    }

    #[test]
    fn rejects_unauthorized_stale_or_mismatched_commitments() {
        let (_, _, genesis, envoy, maker, _) = fixture();
        let args = args_for(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);

        assert_eq!(
            genesis.preview_event(&maker, &args).unwrap_err(),
            ProgramError::Custom(ReceiptError::UnauthorizedActor as u32)
        );

        let mut stale = args.clone();
        stale.expected_seq = 99;
        assert_eq!(
            genesis.preview_event(&envoy, &stale).unwrap_err(),
            ProgramError::Custom(ReceiptError::StaleSequence as u32)
        );

        let mut wrong_rules = args.clone();
        wrong_rules.ruleset_hash = bytes32(88);
        assert_eq!(
            genesis.preview_event(&envoy, &wrong_rules).unwrap_err(),
            ProgramError::Custom(ReceiptError::RulesetMismatch as u32)
        );

        let mut wrong_root = args.clone();
        wrong_root.expected_new_state_root = bytes32(77);
        assert_eq!(
            genesis.preview_event(&envoy, &wrong_root).unwrap_err(),
            ProgramError::Custom(ReceiptError::NewRootMismatch as u32)
        );
    }

    #[test]
    fn account_processor_requires_a_real_signature_and_persists_computed_root() {
        let (program_id, state_pda, genesis, envoy, _, _) = fixture();
        let args = args_for(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);
        let instruction_data =
            borsh::to_vec(&PermutationInstruction::ApplyWorksiteEvent(args)).unwrap();

        let mut actor_lamports = 1;
        let mut actor_data = [];
        let actor_owner = system_program::id();
        let mut state_lamports = 1_000_000;
        let mut state_data = vec![0_u8; ACCOUNT_SPACE];
        let state_owner = program_id;

        {
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &state_owner,
                false,
            );
            write_state(&state_info, &genesis).unwrap();
        }

        let result_without_signature = {
            let actor_info = AccountInfo::new(
                &envoy,
                false,
                false,
                &mut actor_lamports,
                &mut actor_data,
                &actor_owner,
                false,
            );
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &state_owner,
                false,
            );
            process_instruction(&program_id, &[actor_info, state_info], &instruction_data)
        };
        assert_eq!(
            result_without_signature.unwrap_err(),
            ProgramError::Custom(ReceiptError::MissingSignature as u32)
        );

        {
            let actor_info = AccountInfo::new(
                &envoy,
                true,
                false,
                &mut actor_lamports,
                &mut actor_data,
                &actor_owner,
                false,
            );
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &state_owner,
                false,
            );
            process_instruction(&program_id, &[actor_info, state_info], &instruction_data).unwrap();
        }

        let state_info = AccountInfo::new(
            &state_pda,
            false,
            true,
            &mut state_lamports,
            &mut state_data,
            &state_owner,
            false,
        );
        let stored = read_state(&state_info).unwrap();
        assert_eq!(stored.seq, 1);
        assert_eq!(stored.branch, 1);
        assert_eq!(stored.state_root, stored.compute_state_root().unwrap());
    }

    #[test]
    fn er_instruction_variants_append_without_changing_existing_wire_tags() {
        let (_, _, genesis, envoy, _, _) = fixture();
        let genesis_bytes = borsh::to_vec(&genesis).unwrap();
        assert_eq!(genesis_bytes.len(), 333);
        assert!(genesis_bytes.len() <= ACCOUNT_SPACE);
        let apply_args = args_for(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);
        let initialize_args = InitializeArgs {
            season_id: genesis.season_id,
            worksite_id: genesis.worksite_id,
            ruleset_hash: genesis.ruleset_hash,
            envoy: genesis.envoy,
            maker: genesis.maker,
            successor: genesis.successor,
            expected_genesis_state_root: genesis.state_root,
        };

        assert_eq!(
            borsh::to_vec(&PermutationInstruction::Initialize(initialize_args)).unwrap()[0],
            0
        );
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::ApplyWorksiteEvent(apply_args)).unwrap()[0],
            1
        );
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::Delegate).unwrap(),
            vec![2]
        );
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::Commit).unwrap(),
            vec![3]
        );
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::CommitAndUndelegate).unwrap(),
            vec![4]
        );
    }

    #[test]
    fn undelegate_callback_uses_magicblock_discriminator_and_exact_worksite_seeds() {
        let (program_id, state_pda, state, _, _, _) = fixture();
        let seeds = vec![
            WORKSITE_SEED.to_vec(),
            state.season_id.to_vec(),
            state.worksite_id.to_vec(),
        ];
        validate_worksite_identity(&program_id, &state_pda, &state).unwrap();
        validate_stored_state(&state).unwrap();
        validate_undelegate_seeds(&state, &seeds).unwrap();

        let mut callback_data = UNDELEGATE_CALLBACK_DISCRIMINATOR.to_vec();
        callback_data.extend(borsh::to_vec(&seeds).unwrap());
        let rest = callback_data
            .strip_prefix(&UNDELEGATE_CALLBACK_DISCRIMINATOR)
            .unwrap();
        assert_eq!(Vec::<Vec<u8>>::try_from_slice(rest).unwrap(), seeds);

        let mut forged = seeds.clone();
        forged[2][0] ^= 0xff;
        assert_eq!(
            validate_undelegate_seeds(&state, &forged).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidUndelegateSeeds as u32)
        );

        let mut corrupted = state;
        corrupted.water += 1;
        assert_eq!(
            validate_stored_state(&corrupted).unwrap_err(),
            ProgramError::Custom(ReceiptError::StoredRootMismatch as u32)
        );
    }

    #[test]
    fn checkpoint_is_participant_scoped_but_session_end_is_authority_only() {
        let (program_id, state_pda, state, _, maker, _) = fixture();
        assert!(is_worksite_participant(&state, &state.authority));
        assert!(is_worksite_participant(&state, &state.envoy));
        assert!(is_worksite_participant(&state, &state.maker));
        assert!(is_worksite_participant(&state, &state.successor));
        assert!(!is_worksite_participant(&state, &Pubkey::new_unique()));

        let instruction_data = borsh::to_vec(&PermutationInstruction::CommitAndUndelegate).unwrap();
        let mut payer_lamports = 1;
        let mut payer_data = [];
        let payer_owner = system_program::id();
        let mut state_lamports = 1_000_000;
        let mut state_data = vec![0_u8; ACCOUNT_SPACE];
        let mut magic_program_lamports = 1;
        let mut magic_program_data = [];
        let mut magic_context_lamports = 1;
        let mut magic_context_data = [];
        let magic_program_key = MAGIC_PROGRAM_ID;
        let magic_context_key = MAGIC_CONTEXT_ID;

        {
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &program_id,
                false,
            );
            write_state(&state_info, &state).unwrap();
        }

        let result = {
            let payer_info = AccountInfo::new(
                &maker,
                true,
                true,
                &mut payer_lamports,
                &mut payer_data,
                &payer_owner,
                false,
            );
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &program_id,
                false,
            );
            let magic_program_info = AccountInfo::new(
                &magic_program_key,
                false,
                false,
                &mut magic_program_lamports,
                &mut magic_program_data,
                &payer_owner,
                true,
            );
            let magic_context_info = AccountInfo::new(
                &magic_context_key,
                false,
                true,
                &mut magic_context_lamports,
                &mut magic_context_data,
                &payer_owner,
                false,
            );
            process_instruction(
                &program_id,
                &[
                    payer_info,
                    state_info,
                    magic_program_info,
                    magic_context_info,
                ],
                &instruction_data,
            )
        };
        assert_eq!(
            result.unwrap_err(),
            ProgramError::Custom(ReceiptError::UnauthorizedLifecycleActor as u32)
        );
    }

    #[test]
    fn delegate_account_order_reaches_authority_guard_before_cpi() {
        let (program_id, state_pda, state, _, maker, _) = fixture();
        let instruction_data = borsh::to_vec(&PermutationInstruction::Delegate).unwrap();
        let system_key = system_program::id();
        let buffer_key = Pubkey::new_unique();
        let record_key = Pubkey::new_unique();
        let metadata_key = Pubkey::new_unique();

        let mut payer_lamports = 1;
        let mut payer_data = [];
        let mut system_lamports = 1;
        let mut system_data = [];
        let mut state_lamports = 1_000_000;
        let mut state_data = vec![0_u8; ACCOUNT_SPACE];
        let mut owner_program_lamports = 1;
        let mut owner_program_data = [];
        let mut buffer_lamports = 1;
        let mut buffer_data = [];
        let mut record_lamports = 1;
        let mut record_data = [];
        let mut metadata_lamports = 1;
        let mut metadata_data = [];
        let mut delegation_program_lamports = 1;
        let mut delegation_program_data = [];

        {
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &program_id,
                false,
            );
            write_state(&state_info, &state).unwrap();
        }

        let result = {
            let payer_info = AccountInfo::new(
                &maker,
                true,
                true,
                &mut payer_lamports,
                &mut payer_data,
                &system_key,
                false,
            );
            let system_info = AccountInfo::new(
                &system_key,
                false,
                false,
                &mut system_lamports,
                &mut system_data,
                &system_key,
                true,
            );
            let state_info = AccountInfo::new(
                &state_pda,
                false,
                true,
                &mut state_lamports,
                &mut state_data,
                &program_id,
                false,
            );
            let owner_program_info = AccountInfo::new(
                &program_id,
                false,
                false,
                &mut owner_program_lamports,
                &mut owner_program_data,
                &system_key,
                true,
            );
            let buffer_info = AccountInfo::new(
                &buffer_key,
                false,
                true,
                &mut buffer_lamports,
                &mut buffer_data,
                &system_key,
                false,
            );
            let record_info = AccountInfo::new(
                &record_key,
                false,
                true,
                &mut record_lamports,
                &mut record_data,
                &system_key,
                false,
            );
            let metadata_info = AccountInfo::new(
                &metadata_key,
                false,
                true,
                &mut metadata_lamports,
                &mut metadata_data,
                &system_key,
                false,
            );
            let delegation_program_info = AccountInfo::new(
                &DELEGATION_PROGRAM_ID,
                false,
                false,
                &mut delegation_program_lamports,
                &mut delegation_program_data,
                &system_key,
                true,
            );
            process_instruction(
                &program_id,
                &[
                    payer_info,
                    system_info,
                    state_info,
                    owner_program_info,
                    buffer_info,
                    record_info,
                    metadata_info,
                    delegation_program_info,
                ],
                &instruction_data,
            )
        };
        assert_eq!(
            result.unwrap_err(),
            ProgramError::Custom(ReceiptError::UnauthorizedLifecycleActor as u32)
        );
    }

    #[test]
    fn callback_discriminator_is_not_accepted_as_a_normal_borsh_instruction() {
        let seeds = vec![
            WORKSITE_SEED.to_vec(),
            bytes32(7).to_vec(),
            bytes32(31).to_vec(),
        ];
        let mut callback_data = UNDELEGATE_CALLBACK_DISCRIMINATOR.to_vec();
        callback_data.extend(borsh::to_vec(&seeds).unwrap());

        assert!(PermutationInstruction::try_from_slice(&callback_data).is_err());
        assert_eq!(
            process_instruction(&Pubkey::new_unique(), &[], &callback_data).unwrap_err(),
            ProgramError::NotEnoughAccountKeys
        );
    }
}
