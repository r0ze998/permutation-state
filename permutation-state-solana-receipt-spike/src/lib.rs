#![allow(unexpected_cfgs)]

use borsh::{BorshDeserialize, BorshSerialize};
use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
#[cfg(not(feature = "no-entrypoint"))]
use solana_program::entrypoint;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

pub const VERSION: u8 = 1;
pub const WORKSITE_SEED: &[u8] = b"worksite";
pub const SEASON_SEED: &[u8] = b"season";
pub const SEASON_CLAIM_SEED: &[u8] = b"season_claim";
pub const ACCOUNT_SPACE: usize = 384;
pub const SEASON_ACCOUNT_SPACE: usize = 384;
pub const CLAIM_ACCOUNT_SPACE: usize = 160;
pub const STATE_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_STATE/V1";
pub const EVENT_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_EVENT/V1";
pub const GENESIS_DOMAIN: &[u8] = b"PERMSTATE/WORKSITE_GENESIS/V1";
pub const LOG_DOMAIN: &[u8] = b"PERMSTATE_EVENT_V1";
pub const SEASON_STATE_DOMAIN: &[u8] = b"PERMSTATE/SEASON_STATE/V1";
pub const SEASON_GENESIS_DOMAIN: &[u8] = b"PERMSTATE/SEASON_GENESIS/V1";
pub const SEASON_EVENT_DOMAIN: &[u8] = b"PERMSTATE/SEASON_EVENT/V1";
pub const SEASON_LOG_DOMAIN: &[u8] = b"PERMSTATE_SEASON_EVENT_V1";
pub const SEASON_REGISTER_EVENT_DOMAIN: &[u8] = b"PERMSTATE/SEASON_REGISTER/V1";
pub const CLAIM_LEAF_DOMAIN: &[u8] = b"PERMSTATE/SEASON_CLAIM_LEAF/V1";
pub const CLAIM_NODE_DOMAIN: &[u8] = b"PERMSTATE/SEASON_CLAIM_NODE/V1";
pub const UNDELEGATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const ENTRY_PURSE_BPS: u64 = 7_000;
pub const MARKETPLACE_PURSE_BPS: u64 = 150;
pub const MARKETPLACE_OPS_BPS: u64 = 100;
pub const MAX_MOCK_CREDIT_UNITS: u64 = 1_000_000_000_000;
pub const MAX_ACTIVE_WORKSITES: u32 = 8;
pub const MAX_CLAIM_PROOF_DEPTH: usize = 20;

pub const EVENT_MARA_CHOICE: u8 = 0;
pub const EVENT_IVO_CHOICE: u8 = 1;
pub const EVENT_MANDATE_ACCEPTED: u8 = 2;

pub const PURSE_SOURCE_ENTRY: u8 = 1;
pub const PURSE_SOURCE_MARKETPLACE: u8 = 2;

pub const SEASON_EVENT_WORKSITE_REGISTERED: u8 = 0;
pub const SEASON_EVENT_CREDITED: u8 = 1;
pub const SEASON_EVENT_FINALIZED: u8 = 2;
pub const SEASON_EVENT_CLAIMED: u8 = 3;

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
const SEASON_FINALIZED: u8 = 2;
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
pub struct InitializeSeasonArgs {
    pub season_id: [u8; 32],
    pub ruleset_hash: [u8; 32],
    pub payout_rules_hash: [u8; 32],
    pub expected_genesis_state_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CreditSeasonPurseArgs {
    pub source_kind: u8,
    pub gross_units: u64,
    pub expected_seq: u64,
    pub expected_head_event_hash: [u8; 32],
    pub event_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FinalizeSeasonArgs {
    pub expected_seq: u64,
    pub expected_head_event_hash: [u8; 32],
    pub event_id: [u8; 32],
    pub outcome_hash: [u8; 32],
    pub chronicle_root: [u8; 32],
    pub claim_root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ClaimSeasonArgs {
    pub amount: u64,
    pub leaf_index: u32,
    pub merkle_proof: Vec<[u8; 32]>,
    pub expected_seq: u64,
    pub expected_head_event_hash: [u8; 32],
    pub event_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PermutationInstruction {
    Initialize(InitializeArgs),
    ApplyWorksiteEvent(ApplyEventArgs),
    Delegate,
    Commit,
    CommitAndUndelegate,
    InitializeSeason(InitializeSeasonArgs),
    CreditSeasonPurse(CreditSeasonPurseArgs),
    FinalizeSeason(FinalizeSeasonArgs),
    ClaimSeason(ClaimSeasonArgs),
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
    pub season_purse: Pubkey,
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
    season_purse: Pubkey,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeasonState {
    pub version: u8,
    pub bump: u8,
    pub authority: Pubkey,
    pub season_id: [u8; 32],
    pub ruleset_hash: [u8; 32],
    pub payout_rules_hash: [u8; 32],
    pub outcome_hash: [u8; 32],
    pub chronicle_root: [u8; 32],
    pub claim_root: [u8; 32],
    pub state_root: [u8; 32],
    pub head_event_hash: [u8; 32],
    pub status: u8,
    pub seq: u64,
    pub active_worksites: u32,
    pub active_citizens: u32,
    pub entry_gross_units: u64,
    pub entry_purse_units: u64,
    pub marketplace_gross_units: u64,
    pub marketplace_purse_units: u64,
    pub seller_units: u64,
    pub ops_units: u64,
    pub purse_total: u64,
    pub claimable_units: u64,
    pub claimed_units: u64,
    pub claim_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
struct SeasonStateRootView {
    version: u8,
    bump: u8,
    authority: Pubkey,
    season_id: [u8; 32],
    ruleset_hash: [u8; 32],
    payout_rules_hash: [u8; 32],
    outcome_hash: [u8; 32],
    chronicle_root: [u8; 32],
    claim_root: [u8; 32],
    status: u8,
    seq: u64,
    active_worksites: u32,
    active_citizens: u32,
    entry_gross_units: u64,
    entry_purse_units: u64,
    marketplace_gross_units: u64,
    marketplace_purse_units: u64,
    seller_units: u64,
    ops_units: u64,
    purse_total: u64,
    claimable_units: u64,
    claimed_units: u64,
    claim_count: u32,
}

struct SeasonEventInput {
    event_kind: u8,
    source_kind: u8,
    actor: Pubkey,
    subject: Pubkey,
    gross_or_claim_units: u64,
    event_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeasonClaimReceipt {
    pub version: u8,
    pub bump: u8,
    pub season: Pubkey,
    pub claimant: Pubkey,
    pub leaf_index: u32,
    pub amount: u64,
    pub leaf_hash: [u8; 32],
    pub season_event_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeasonReceiptV1 {
    pub version: u8,
    pub seq: u64,
    pub event_kind: u8,
    pub source_kind: u8,
    pub actor: Pubkey,
    pub subject: Pubkey,
    pub gross_or_claim_units: u64,
    pub prior_state_root: [u8; 32],
    pub new_state_root: [u8; 32],
    pub previous_event_hash: [u8; 32],
    pub event_hash: [u8; 32],
    pub event_id: [u8; 32],
    pub purse_total: u64,
    pub claimable_units: u64,
    pub claimed_units: u64,
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
    InvalidInitialization = 23,
    InvalidSeasonPda = 24,
    SeasonNotActive = 25,
    InvalidPurseSource = 26,
    InvalidAmount = 27,
    PurseOverflow = 28,
    StaleSeasonSequence = 29,
    SeasonHeadMismatch = 30,
    InvalidEventId = 31,
    SeasonHasActiveWork = 32,
    InvalidFinalizeAccounts = 33,
    SeasonNotFinalized = 34,
    InvalidClaimPda = 35,
    InvalidMerkleProof = 36,
    ClaimExceedsPurse = 37,
    ClaimAlreadyExists = 38,
    InvalidWorksiteState = 39,
    InvalidFinalization = 40,
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
        validate_worksite_initialize_args(authority, args)?;
        let (season_purse, _) =
            Pubkey::find_program_address(&[SEASON_SEED, &args.season_id], program_id);
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
            season_purse,
        };
        state.assert_invariants()?;
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
            season_purse: self.season_purse,
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
        if self.food != 46 || self.season_purse == Pubkey::default() {
            return Err(ReceiptError::InvalidWorksiteState.into());
        }

        let valid = match self.stage {
            STAGE_MARA => {
                self.seq == 0
                    && self.branch == 0
                    && self.resolution == 0
                    && self.water == 28
                    && self.cohesion == 54
                    && self.timber == 18
                    && self.prosperity == 38
                    && self.food_debt == 0
                    && self.tala_trust == 0
                    && self.service_stair == 0
                    && self.riverkeepers == 0
                    && self.memory_receipt == 0
                    && self.worksite_status == 0
                    && self.mandate_ids == [0; 3]
                    && self.mandate_status == [0; 3]
                    && self.accepted_mandate == 0
            }
            STAGE_IVO => {
                self.seq == 1
                    && self.resolution == 0
                    && self.timber == 18
                    && self.prosperity == 38
                    && self.worksite_status == 1
                    && self.mandate_ids == [0; 3]
                    && self.mandate_status == [0; 3]
                    && self.accepted_mandate == 0
                    && match self.branch {
                        1 => {
                            self.water == 36
                                && self.cohesion == 60
                                && self.food_debt == 12
                                && self.tala_trust == 20
                                && self.service_stair == 1
                                && self.riverkeepers == 1
                                && self.memory_receipt == 1
                        }
                        2 => {
                            self.water == 33
                                && self.cohesion == 46
                                && self.food_debt == 0
                                && self.tala_trust == -20
                                && self.service_stair == 2
                                && self.riverkeepers == 2
                                && self.memory_receipt == 2
                        }
                        _ => false,
                    }
            }
            STAGE_SUCCESSOR | STAGE_CONTINUING => {
                let stage_fields_valid = if self.stage == STAGE_SUCCESSOR {
                    self.seq == 2 && self.mandate_status == [0; 3] && self.accepted_mandate == 0
                } else {
                    self.seq == 3
                        && self.accepted_mandate != 0
                        && self
                            .mandate_ids
                            .iter()
                            .zip(self.mandate_status.iter())
                            .filter(|(id, status)| **id == self.accepted_mandate && **status == 1)
                            .count()
                            == 1
                        && self
                            .mandate_status
                            .iter()
                            .filter(|status| **status == 1)
                            .count()
                            == 1
                };
                stage_fields_valid && self.matches_resolution_snapshot()
            }
            _ => false,
        };
        if !valid {
            return Err(ReceiptError::InvalidWorksiteState.into());
        }
        Ok(())
    }

    fn matches_resolution_snapshot(&self) -> bool {
        match (self.branch, self.resolution) {
            (1, 1) => {
                self.water == 60
                    && self.cohesion == 64
                    && self.timber == 12
                    && self.prosperity == 43
                    && self.food_debt == 12
                    && self.tala_trust == 28
                    && self.service_stair == 1
                    && self.riverkeepers == 1
                    && self.memory_receipt == 1
                    && self.worksite_status == 2
                    && self.mandate_ids == [MANDATE_M_032, MANDATE_E_019, MANDATE_S_044]
            }
            (1, 2) => {
                self.water == 54
                    && self.cohesion == 56
                    && self.timber == 15
                    && self.prosperity == 41
                    && self.food_debt == 12
                    && self.tala_trust == 8
                    && self.service_stair == 1
                    && self.riverkeepers == 1
                    && self.memory_receipt == 1
                    && self.worksite_status == 2
                    && self.mandate_ids == [MANDATE_E_020, MANDATE_M_032, MANDATE_S_045]
            }
            (2, 3) => {
                self.water == 55
                    && self.cohesion == 41
                    && self.timber == 8
                    && self.prosperity == 39
                    && self.food_debt == 0
                    && self.tala_trust == -35
                    && self.service_stair == 2
                    && self.riverkeepers == 2
                    && self.memory_receipt == 2
                    && self.worksite_status == 3
                    && self.mandate_ids == [MANDATE_E_021, MANDATE_W_014, MANDATE_S_046]
            }
            (2, 4) => {
                self.water == 51
                    && self.cohesion == 54
                    && self.timber == 12
                    && self.prosperity == 42
                    && self.food_debt == 8
                    && self.tala_trust == 5
                    && self.service_stair == 1
                    && self.riverkeepers == 3
                    && self.memory_receipt == 2
                    && self.worksite_status == 2
                    && self.mandate_ids == [MANDATE_M_033, MANDATE_E_022, MANDATE_S_047]
            }
            _ => false,
        }
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
        if is_zero_hash(&args.client_event_hash) {
            return Err(ReceiptError::InvalidEventId.into());
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

impl SeasonState {
    pub fn new(
        program_id: &Pubkey,
        state_pda: &Pubkey,
        bump: u8,
        authority: Pubkey,
        args: &InitializeSeasonArgs,
    ) -> Result<Self, ProgramError> {
        if authority == Pubkey::default()
            || is_zero_hash(&args.season_id)
            || is_zero_hash(&args.ruleset_hash)
            || is_zero_hash(&args.payout_rules_hash)
        {
            return Err(ReceiptError::InvalidInitialization.into());
        }

        let mut state = Self {
            version: VERSION,
            bump,
            authority,
            season_id: args.season_id,
            ruleset_hash: args.ruleset_hash,
            payout_rules_hash: args.payout_rules_hash,
            outcome_hash: [0; 32],
            chronicle_root: [0; 32],
            claim_root: [0; 32],
            state_root: [0; 32],
            head_event_hash: [0; 32],
            status: SEASON_ACTIVE,
            seq: 0,
            active_worksites: 0,
            active_citizens: 0,
            entry_gross_units: 0,
            entry_purse_units: 0,
            marketplace_gross_units: 0,
            marketplace_purse_units: 0,
            seller_units: 0,
            ops_units: 0,
            purse_total: 0,
            claimable_units: 0,
            claimed_units: 0,
            claim_count: 0,
        };
        state.assert_invariants()?;
        state.state_root = state.compute_state_root()?;
        state.head_event_hash = hashv(&[
            SEASON_GENESIS_DOMAIN,
            program_id.as_ref(),
            state_pda.as_ref(),
            &state.season_id,
            &state.ruleset_hash,
            &state.payout_rules_hash,
            &state.state_root,
        ])
        .to_bytes();
        Ok(state)
    }

    fn root_view(&self) -> SeasonStateRootView {
        SeasonStateRootView {
            version: self.version,
            bump: self.bump,
            authority: self.authority,
            season_id: self.season_id,
            ruleset_hash: self.ruleset_hash,
            payout_rules_hash: self.payout_rules_hash,
            outcome_hash: self.outcome_hash,
            chronicle_root: self.chronicle_root,
            claim_root: self.claim_root,
            status: self.status,
            seq: self.seq,
            active_worksites: self.active_worksites,
            active_citizens: self.active_citizens,
            entry_gross_units: self.entry_gross_units,
            entry_purse_units: self.entry_purse_units,
            marketplace_gross_units: self.marketplace_gross_units,
            marketplace_purse_units: self.marketplace_purse_units,
            seller_units: self.seller_units,
            ops_units: self.ops_units,
            purse_total: self.purse_total,
            claimable_units: self.claimable_units,
            claimed_units: self.claimed_units,
            claim_count: self.claim_count,
        }
    }

    pub fn compute_state_root(&self) -> Result<[u8; 32], ProgramError> {
        let bytes = borsh::to_vec(&self.root_view()).map_err(|_| ReceiptError::StateEncoding)?;
        Ok(hashv(&[SEASON_STATE_DOMAIN, &bytes]).to_bytes())
    }

    pub fn assert_invariants(&self) -> ProgramResult {
        if self.version != VERSION
            || self.authority == Pubkey::default()
            || is_zero_hash(&self.season_id)
            || is_zero_hash(&self.ruleset_hash)
            || is_zero_hash(&self.payout_rules_hash)
            || self.active_worksites > MAX_ACTIVE_WORKSITES
        {
            return Err(ReceiptError::InvariantViolation.into());
        }

        let expected_active_citizens = self
            .active_worksites
            .checked_mul(3)
            .ok_or(ReceiptError::PurseOverflow)?;
        if self.active_citizens != expected_active_citizens {
            return Err(ReceiptError::InvariantViolation.into());
        }

        let expected_entry_purse = bps_floor(self.entry_gross_units, ENTRY_PURSE_BPS)?;
        let expected_marketplace_purse =
            bps_floor(self.marketplace_gross_units, MARKETPLACE_PURSE_BPS)?;
        let expected_marketplace_ops =
            bps_floor(self.marketplace_gross_units, MARKETPLACE_OPS_BPS)?;
        let expected_seller = self
            .marketplace_gross_units
            .checked_sub(expected_marketplace_purse)
            .and_then(|value| value.checked_sub(expected_marketplace_ops))
            .ok_or(ReceiptError::PurseOverflow)?;
        let expected_entry_ops = self
            .entry_gross_units
            .checked_sub(expected_entry_purse)
            .ok_or(ReceiptError::PurseOverflow)?;
        let expected_ops = expected_entry_ops
            .checked_add(expected_marketplace_ops)
            .ok_or(ReceiptError::PurseOverflow)?;
        let expected_purse_total = expected_entry_purse
            .checked_add(expected_marketplace_purse)
            .ok_or(ReceiptError::PurseOverflow)?;
        let total_gross = self
            .entry_gross_units
            .checked_add(self.marketplace_gross_units)
            .ok_or(ReceiptError::PurseOverflow)?;
        let total_allocated = self
            .purse_total
            .checked_add(self.ops_units)
            .and_then(|value| value.checked_add(self.seller_units))
            .ok_or(ReceiptError::PurseOverflow)?;
        if self.entry_purse_units != expected_entry_purse
            || self.marketplace_purse_units != expected_marketplace_purse
            || self.seller_units != expected_seller
            || self.ops_units != expected_ops
            || self.purse_total != expected_purse_total
            || total_allocated != total_gross
        {
            return Err(ReceiptError::InvariantViolation.into());
        }

        match self.status {
            SEASON_ACTIVE => {
                if self.claimable_units != 0
                    || self.claimed_units != 0
                    || self.claim_count != 0
                    || !is_zero_hash(&self.outcome_hash)
                    || !is_zero_hash(&self.chronicle_root)
                    || !is_zero_hash(&self.claim_root)
                {
                    return Err(ReceiptError::InvariantViolation.into());
                }
            }
            SEASON_FINALIZED => {
                if self.active_worksites != 0 || self.active_citizens != 0 {
                    return Err(ReceiptError::SeasonHasActiveWork.into());
                }
                if self
                    .claimable_units
                    .checked_add(self.claimed_units)
                    .ok_or(ReceiptError::PurseOverflow)?
                    != self.purse_total
                {
                    return Err(ReceiptError::InvariantViolation.into());
                }
                if (self.claim_count == 0) != (self.claimed_units == 0) {
                    return Err(ReceiptError::InvariantViolation.into());
                }
                if is_zero_hash(&self.outcome_hash)
                    || is_zero_hash(&self.chronicle_root)
                    || is_zero_hash(&self.claim_root)
                {
                    return Err(ReceiptError::InvariantViolation.into());
                }
            }
            _ => return Err(ReceiptError::InvariantViolation.into()),
        }
        Ok(())
    }

    fn require_active(&self) -> ProgramResult {
        if self.status != SEASON_ACTIVE {
            return Err(ReceiptError::SeasonNotActive.into());
        }
        Ok(())
    }

    fn require_authority(&self, actor: &Pubkey) -> ProgramResult {
        if actor != &self.authority {
            return Err(ReceiptError::UnauthorizedLifecycleActor.into());
        }
        Ok(())
    }

    fn require_expected_head(
        &self,
        expected_seq: u64,
        expected_head_event_hash: &[u8; 32],
        event_id: &[u8; 32],
    ) -> ProgramResult {
        if expected_seq != self.seq {
            return Err(ReceiptError::StaleSeasonSequence.into());
        }
        if expected_head_event_hash != &self.head_event_hash {
            return Err(ReceiptError::SeasonHeadMismatch.into());
        }
        if is_zero_hash(event_id) {
            return Err(ReceiptError::InvalidEventId.into());
        }
        Ok(())
    }

    fn finish_transition(
        &self,
        mut next: Self,
        event: SeasonEventInput,
    ) -> Result<(Self, SeasonReceiptV1), ProgramError> {
        next.seq = self.seq.checked_add(1).ok_or(ReceiptError::PurseOverflow)?;
        next.assert_invariants()?;
        let new_state_root = next.compute_state_root()?;
        let seq_bytes = self.seq.to_le_bytes();
        let event_kind_bytes = [event.event_kind];
        let source_kind_bytes = [event.source_kind];
        let amount_bytes = event.gross_or_claim_units.to_le_bytes();
        let event_hash = hashv(&[
            SEASON_EVENT_DOMAIN,
            &self.season_id,
            &self.ruleset_hash,
            &self.head_event_hash,
            &self.state_root,
            &new_state_root,
            event.actor.as_ref(),
            event.subject.as_ref(),
            &seq_bytes,
            &event_kind_bytes,
            &source_kind_bytes,
            &amount_bytes,
            &event.event_id,
        ])
        .to_bytes();
        next.state_root = new_state_root;
        next.head_event_hash = event_hash;
        let receipt = SeasonReceiptV1 {
            version: VERSION,
            seq: self.seq,
            event_kind: event.event_kind,
            source_kind: event.source_kind,
            actor: event.actor,
            subject: event.subject,
            gross_or_claim_units: event.gross_or_claim_units,
            prior_state_root: self.state_root,
            new_state_root,
            previous_event_hash: self.head_event_hash,
            event_hash,
            event_id: event.event_id,
            purse_total: next.purse_total,
            claimable_units: next.claimable_units,
            claimed_units: next.claimed_units,
        };
        Ok((next, receipt))
    }

    fn register_worksite(
        &self,
        actor: &Pubkey,
        worksite: &Pubkey,
    ) -> Result<(Self, SeasonReceiptV1), ProgramError> {
        self.assert_invariants()?;
        self.require_active()?;
        self.require_authority(actor)?;
        if self.active_worksites >= MAX_ACTIVE_WORKSITES {
            return Err(ReceiptError::SeasonHasActiveWork.into());
        }
        let mut next = self.clone();
        next.active_worksites = next
            .active_worksites
            .checked_add(1)
            .ok_or(ReceiptError::PurseOverflow)?;
        next.active_citizens = next
            .active_citizens
            .checked_add(3)
            .ok_or(ReceiptError::PurseOverflow)?;
        let event_id = hashv(&[
            SEASON_REGISTER_EVENT_DOMAIN,
            self.season_id.as_ref(),
            worksite.as_ref(),
        ])
        .to_bytes();
        self.finish_transition(
            next,
            SeasonEventInput {
                event_kind: SEASON_EVENT_WORKSITE_REGISTERED,
                source_kind: 0,
                actor: *actor,
                subject: *worksite,
                gross_or_claim_units: 0,
                event_id,
            },
        )
    }

    pub fn credit_purse(
        &self,
        actor: &Pubkey,
        args: &CreditSeasonPurseArgs,
    ) -> Result<(Self, SeasonReceiptV1), ProgramError> {
        self.assert_invariants()?;
        self.require_active()?;
        self.require_authority(actor)?;
        self.require_expected_head(
            args.expected_seq,
            &args.expected_head_event_hash,
            &args.event_id,
        )?;
        if args.gross_units == 0 || args.gross_units > MAX_MOCK_CREDIT_UNITS {
            return Err(ReceiptError::InvalidAmount.into());
        }

        let mut next = self.clone();
        match args.source_kind {
            PURSE_SOURCE_ENTRY => {
                next.entry_gross_units = next
                    .entry_gross_units
                    .checked_add(args.gross_units)
                    .ok_or(ReceiptError::PurseOverflow)?;
            }
            PURSE_SOURCE_MARKETPLACE => {
                next.marketplace_gross_units = next
                    .marketplace_gross_units
                    .checked_add(args.gross_units)
                    .ok_or(ReceiptError::PurseOverflow)?;
            }
            _ => return Err(ReceiptError::InvalidPurseSource.into()),
        }
        next.recompute_allocations()?;
        self.finish_transition(
            next,
            SeasonEventInput {
                event_kind: SEASON_EVENT_CREDITED,
                source_kind: args.source_kind,
                actor: *actor,
                subject: *actor,
                gross_or_claim_units: args.gross_units,
                event_id: args.event_id,
            },
        )
    }

    fn recompute_allocations(&mut self) -> ProgramResult {
        self.entry_purse_units = bps_floor(self.entry_gross_units, ENTRY_PURSE_BPS)?;
        self.marketplace_purse_units =
            bps_floor(self.marketplace_gross_units, MARKETPLACE_PURSE_BPS)?;
        let marketplace_ops = bps_floor(self.marketplace_gross_units, MARKETPLACE_OPS_BPS)?;
        self.seller_units = self
            .marketplace_gross_units
            .checked_sub(self.marketplace_purse_units)
            .and_then(|value| value.checked_sub(marketplace_ops))
            .ok_or(ReceiptError::PurseOverflow)?;
        let entry_ops = self
            .entry_gross_units
            .checked_sub(self.entry_purse_units)
            .ok_or(ReceiptError::PurseOverflow)?;
        self.ops_units = entry_ops
            .checked_add(marketplace_ops)
            .ok_or(ReceiptError::PurseOverflow)?;
        self.purse_total = self
            .entry_purse_units
            .checked_add(self.marketplace_purse_units)
            .ok_or(ReceiptError::PurseOverflow)?;
        Ok(())
    }

    fn finalize(
        &self,
        actor: &Pubkey,
        args: &FinalizeSeasonArgs,
    ) -> Result<(Self, SeasonReceiptV1), ProgramError> {
        self.assert_invariants()?;
        self.require_active()?;
        self.require_authority(actor)?;
        self.require_expected_head(
            args.expected_seq,
            &args.expected_head_event_hash,
            &args.event_id,
        )?;
        if self.active_worksites == 0 || self.active_citizens == 0 || self.purse_total == 0 {
            return Err(ReceiptError::SeasonHasActiveWork.into());
        }
        if is_zero_hash(&args.outcome_hash)
            || is_zero_hash(&args.chronicle_root)
            || is_zero_hash(&args.claim_root)
        {
            return Err(ReceiptError::InvalidFinalization.into());
        }

        let mut next = self.clone();
        next.status = SEASON_FINALIZED;
        next.active_worksites = 0;
        next.active_citizens = 0;
        next.outcome_hash = args.outcome_hash;
        next.chronicle_root = args.chronicle_root;
        next.claim_root = args.claim_root;
        next.claimable_units = next.purse_total;
        self.finish_transition(
            next,
            SeasonEventInput {
                event_kind: SEASON_EVENT_FINALIZED,
                source_kind: 0,
                actor: *actor,
                subject: *actor,
                gross_or_claim_units: self.purse_total,
                event_id: args.event_id,
            },
        )
    }

    fn claim(
        &self,
        claimant: &Pubkey,
        args: &ClaimSeasonArgs,
    ) -> Result<(Self, SeasonReceiptV1, [u8; 32]), ProgramError> {
        self.assert_invariants()?;
        if self.status != SEASON_FINALIZED {
            return Err(ReceiptError::SeasonNotFinalized.into());
        }
        if self.active_worksites != 0 || self.active_citizens != 0 {
            return Err(ReceiptError::SeasonHasActiveWork.into());
        }
        self.require_expected_head(
            args.expected_seq,
            &args.expected_head_event_hash,
            &args.event_id,
        )?;
        if args.amount == 0 {
            return Err(ReceiptError::InvalidAmount.into());
        }
        if args.amount > self.claimable_units {
            return Err(ReceiptError::ClaimExceedsPurse.into());
        }
        let leaf_hash = claim_leaf_hash(&self.season_id, claimant, args.leaf_index, args.amount);
        if !verify_claim_proof(
            leaf_hash,
            args.leaf_index,
            &args.merkle_proof,
            &self.claim_root,
        ) {
            return Err(ReceiptError::InvalidMerkleProof.into());
        }

        let mut next = self.clone();
        next.claimable_units = next
            .claimable_units
            .checked_sub(args.amount)
            .ok_or(ReceiptError::ClaimExceedsPurse)?;
        next.claimed_units = next
            .claimed_units
            .checked_add(args.amount)
            .ok_or(ReceiptError::PurseOverflow)?;
        next.claim_count = next
            .claim_count
            .checked_add(1)
            .ok_or(ReceiptError::PurseOverflow)?;
        let (next, receipt) = self.finish_transition(
            next,
            SeasonEventInput {
                event_kind: SEASON_EVENT_CLAIMED,
                source_kind: 0,
                actor: *claimant,
                subject: *claimant,
                gross_or_claim_units: args.amount,
                event_id: args.event_id,
            },
        )?;
        Ok((next, receipt, leaf_hash))
    }
}

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

#[inline(never)]
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
        PermutationInstruction::InitializeSeason(args) => {
            process_initialize_season(program_id, accounts, args)
        }
        PermutationInstruction::CreditSeasonPurse(args) => {
            process_credit_season_purse(program_id, accounts, args)
        }
        PermutationInstruction::FinalizeSeason(args) => {
            process_finalize_season(program_id, accounts, args)
        }
        PermutationInstruction::ClaimSeason(args) => {
            process_claim_season(program_id, accounts, args)
        }
    }
}

#[inline(never)]
fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: InitializeArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let state_account = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;
    let season_account = next_account_info(&mut accounts)?;

    if !authority.is_signer {
        return Err(ReceiptError::MissingSignature.into());
    }
    if !authority.is_writable || !state_account.is_writable || !season_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    validate_worksite_initialize_args(*authority.key, &args)?;
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[WORKSITE_SEED, &args.season_id, &args.worksite_id],
        program_id,
    );
    if state_account.key != &expected_pda {
        return Err(ReceiptError::WrongPda.into());
    }
    if state_account.owner != &system_program::id() || !state_account.data_is_empty() {
        return Err(ReceiptError::AlreadyInitialized.into());
    }

    let season = read_owned_season_state(program_id, season_account)?;
    validate_season_identity(program_id, season_account.key, &season)?;
    if season_account.key
        != &Pubkey::find_program_address(&[SEASON_SEED, &args.season_id], program_id).0
    {
        return Err(ReceiptError::InvalidSeasonPda.into());
    }
    if season.authority != *authority.key {
        return Err(ReceiptError::UnauthorizedLifecycleActor.into());
    }
    if season.season_id != args.season_id {
        return Err(ReceiptError::InvalidSeasonPda.into());
    }
    if season.ruleset_hash != args.ruleset_hash {
        return Err(ReceiptError::RulesetMismatch.into());
    }
    let (next_season, season_receipt) =
        season.register_worksite(authority.key, state_account.key)?;

    let bump_seed = [bump];
    let signer_seeds: &[&[u8]] = &[
        WORKSITE_SEED,
        &args.season_id,
        &args.worksite_id,
        &bump_seed,
    ];
    initialize_pda_account(
        authority,
        state_account,
        system,
        ACCOUNT_SPACE,
        program_id,
        signer_seeds,
    )?;

    let state = WorksiteState::new(program_id, state_account.key, bump, *authority.key, &args)?;
    if state.state_root != args.expected_genesis_state_root {
        return Err(ReceiptError::NewRootMismatch.into());
    }
    write_state(state_account, &state)?;
    write_season_state(season_account, &next_season)?;
    log_season_receipt(&season_receipt)?;
    msg!("PERMSTATE initialized seq=0");
    Ok(())
}

#[inline(never)]
fn process_initialize_season(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: InitializeSeasonArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let season_account = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;

    require_signer(authority)?;
    if !authority.is_writable || !season_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let (expected_pda, bump) =
        Pubkey::find_program_address(&[SEASON_SEED, &args.season_id], program_id);
    if season_account.key != &expected_pda {
        return Err(ReceiptError::InvalidSeasonPda.into());
    }
    if season_account.owner != &system_program::id() || !season_account.data_is_empty() {
        return Err(ReceiptError::AlreadyInitialized.into());
    }

    let state = SeasonState::new(program_id, season_account.key, bump, *authority.key, &args)?;
    if state.state_root != args.expected_genesis_state_root {
        return Err(ReceiptError::NewRootMismatch.into());
    }

    let bump_seed = [bump];
    let signer_seeds: &[&[u8]] = &[SEASON_SEED, &args.season_id, &bump_seed];
    initialize_pda_account(
        authority,
        season_account,
        system,
        SEASON_ACCOUNT_SPACE,
        program_id,
        signer_seeds,
    )?;
    write_season_state(season_account, &state)?;
    msg!("PERMSTATE season initialized seq=0");
    Ok(())
}

#[inline(never)]
fn process_credit_season_purse(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: CreditSeasonPurseArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let season_account = next_account_info(&mut accounts)?;

    require_signer(authority)?;
    if !season_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    let season = read_owned_season_state(program_id, season_account)?;
    let (next, receipt) = season.credit_purse(authority.key, &args)?;
    write_season_state(season_account, &next)?;
    log_season_receipt(&receipt)?;
    msg!(
        "PERMSTATE mock purse credit seq={} source={} gross={}",
        receipt.seq,
        receipt.source_kind,
        receipt.gross_or_claim_units
    );
    Ok(())
}

#[inline(never)]
fn process_finalize_season(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: FinalizeSeasonArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let authority = next_account_info(&mut accounts)?;
    let season_account = next_account_info(&mut accounts)?;
    let worksite_accounts: Vec<&AccountInfo> = accounts.collect();

    require_signer(authority)?;
    if !season_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    let season = read_owned_season_state(program_id, season_account)?;
    season.require_authority(authority.key)?;
    if worksite_accounts.len() != season.active_worksites as usize
        || worksite_accounts.is_empty()
        || worksite_accounts.len() > MAX_ACTIVE_WORKSITES as usize
    {
        return Err(ReceiptError::InvalidFinalizeAccounts.into());
    }

    for (index, account) in worksite_accounts.iter().enumerate() {
        if worksite_accounts[..index]
            .iter()
            .any(|prior| prior.key == account.key)
        {
            return Err(ReceiptError::InvalidFinalizeAccounts.into());
        }
        let worksite = read_owned_worksite_state(program_id, account)?;
        if worksite.season_purse != *season_account.key
            || worksite.season_id != season.season_id
            || worksite.ruleset_hash != season.ruleset_hash
            || worksite.stage != STAGE_CONTINUING
            || worksite.seq != 3
            || worksite.accepted_mandate == 0
        {
            return Err(ReceiptError::InvalidFinalizeAccounts.into());
        }
    }

    let (next, receipt) = season.finalize(authority.key, &args)?;
    write_season_state(season_account, &next)?;
    log_season_receipt(&receipt)?;
    msg!(
        "PERMSTATE season finalized seq={} purse={}",
        receipt.seq,
        receipt.purse_total
    );
    Ok(())
}

#[inline(never)]
fn process_claim_season(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: ClaimSeasonArgs,
) -> ProgramResult {
    let mut accounts = accounts.iter();
    let claimant = next_account_info(&mut accounts)?;
    let season_account = next_account_info(&mut accounts)?;
    let claim_account = next_account_info(&mut accounts)?;
    let system = next_account_info(&mut accounts)?;

    require_signer(claimant)?;
    if !claimant.is_writable || !season_account.is_writable || !claim_account.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    if system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let season = read_owned_season_state(program_id, season_account)?;
    let (expected_claim_pda, bump) = Pubkey::find_program_address(
        &[SEASON_CLAIM_SEED, &season.season_id, claimant.key.as_ref()],
        program_id,
    );
    if claim_account.key != &expected_claim_pda {
        return Err(ReceiptError::InvalidClaimPda.into());
    }
    if claim_account.owner != &system_program::id() || !claim_account.data_is_empty() {
        return Err(ReceiptError::ClaimAlreadyExists.into());
    }

    let (next, receipt, leaf_hash) = season.claim(claimant.key, &args)?;
    let bump_seed = [bump];
    let signer_seeds: &[&[u8]] = &[
        SEASON_CLAIM_SEED,
        &season.season_id,
        claimant.key.as_ref(),
        &bump_seed,
    ];
    initialize_pda_account(
        claimant,
        claim_account,
        system,
        CLAIM_ACCOUNT_SPACE,
        program_id,
        signer_seeds,
    )?;
    let claim_receipt = SeasonClaimReceipt {
        version: VERSION,
        bump,
        season: *season_account.key,
        claimant: *claimant.key,
        leaf_index: args.leaf_index,
        amount: args.amount,
        leaf_hash,
        season_event_hash: receipt.event_hash,
    };
    write_claim_receipt(claim_account, &claim_receipt)?;
    write_season_state(season_account, &next)?;
    log_season_receipt(&receipt)?;
    msg!(
        "PERMSTATE season claim seq={} amount={}",
        receipt.seq,
        receipt.gross_or_claim_units
    );
    Ok(())
}

/// Base-layer instruction. Delegates the canonical Worksite PDA to the
/// validator selected by the client (or MagicBlock's default when omitted).
#[inline(never)]
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
#[inline(never)]
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
#[inline(never)]
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
#[inline(never)]
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

#[inline(never)]
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

/// Claims a system-owned PDA even when a third party has prefunded it. This
/// avoids the common one-lamport PDA-squatting denial of service while still
/// requiring an empty, system-owned account at the canonical address.
fn initialize_pda_account<'info>(
    payer: &AccountInfo<'info>,
    pda: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    space: usize,
    owner: &Pubkey,
    signer_seeds: &[&[u8]],
) -> ProgramResult {
    if pda.owner != &system_program::id() || !pda.data_is_empty() {
        return Err(ReceiptError::AlreadyInitialized.into());
    }
    let required_lamports = Rent::get()?.minimum_balance(space);
    let current_lamports = pda.lamports();
    if current_lamports < required_lamports {
        invoke(
            &system_instruction::transfer(payer.key, pda.key, required_lamports - current_lamports),
            &[payer.clone(), pda.clone(), system.clone()],
        )?;
    }
    invoke_signed(
        &system_instruction::allocate(pda.key, space as u64),
        &[pda.clone(), system.clone()],
        &[signer_seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(pda.key, owner),
        &[pda.clone(), system.clone()],
        &[signer_seeds],
    )?;
    Ok(())
}

fn is_worksite_participant(state: &WorksiteState, key: &Pubkey) -> bool {
    key == &state.authority || key == &state.envoy || key == &state.maker || key == &state.successor
}

fn is_zero_hash(value: &[u8; 32]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

fn validate_worksite_initialize_args(authority: Pubkey, args: &InitializeArgs) -> ProgramResult {
    if authority == Pubkey::default()
        || is_zero_hash(&args.season_id)
        || is_zero_hash(&args.worksite_id)
        || is_zero_hash(&args.ruleset_hash)
        || args.envoy == Pubkey::default()
        || args.maker == Pubkey::default()
        || args.successor == Pubkey::default()
        || args.envoy == args.maker
        || args.envoy == args.successor
        || args.maker == args.successor
    {
        return Err(ReceiptError::InvalidInitialization.into());
    }
    Ok(())
}

fn bps_floor(gross_units: u64, basis_points: u64) -> Result<u64, ProgramError> {
    let units = (gross_units as u128)
        .checked_mul(basis_points as u128)
        .ok_or(ReceiptError::PurseOverflow)?
        / BPS_DENOMINATOR as u128;
    u64::try_from(units).map_err(|_| ReceiptError::PurseOverflow.into())
}

pub fn claim_leaf_hash(
    season_id: &[u8; 32],
    claimant: &Pubkey,
    leaf_index: u32,
    amount: u64,
) -> [u8; 32] {
    hashv(&[
        CLAIM_LEAF_DOMAIN,
        season_id,
        claimant.as_ref(),
        &leaf_index.to_le_bytes(),
        &amount.to_le_bytes(),
    ])
    .to_bytes()
}

fn verify_claim_proof(
    mut current: [u8; 32],
    leaf_index: u32,
    proof: &[[u8; 32]],
    expected_root: &[u8; 32],
) -> bool {
    if proof.len() > MAX_CLAIM_PROOF_DEPTH || (leaf_index >> proof.len()) != 0 {
        return false;
    }
    for (depth, sibling) in proof.iter().enumerate() {
        current = if ((leaf_index >> depth) & 1) == 0 {
            hashv(&[CLAIM_NODE_DOMAIN, &current, sibling]).to_bytes()
        } else {
            hashv(&[CLAIM_NODE_DOMAIN, sibling, &current]).to_bytes()
        };
    }
    &current == expected_root
}

fn log_season_receipt(receipt: &SeasonReceiptV1) -> ProgramResult {
    let receipt_bytes = borsh::to_vec(receipt).map_err(|_| ReceiptError::StateEncoding)?;
    solana_program::log::sol_log_data(&[SEASON_LOG_DOMAIN, &receipt_bytes]);
    Ok(())
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

fn read_owned_season_state(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<SeasonState, ProgramError> {
    if account.owner != program_id {
        return Err(ReceiptError::WrongOwner.into());
    }
    let state = read_season_state(account)?;
    validate_season_identity(program_id, account.key, &state)?;
    validate_stored_season_state(program_id, account.key, &state)?;
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
    let expected_season_purse =
        Pubkey::find_program_address(&[SEASON_SEED, &state.season_id], program_id).0;
    if state.season_purse != expected_season_purse {
        return Err(ReceiptError::InvalidSeasonPda.into());
    }
    if state.seq == 0 {
        let expected_genesis_head = hashv(&[
            GENESIS_DOMAIN,
            program_id.as_ref(),
            state_key.as_ref(),
            &state.ruleset_hash,
            &state.state_root,
        ])
        .to_bytes();
        if state.head_event_hash != expected_genesis_head {
            return Err(ReceiptError::StoredRootMismatch.into());
        }
    } else if is_zero_hash(&state.head_event_hash) {
        return Err(ReceiptError::StoredRootMismatch.into());
    }
    Ok(())
}

fn validate_season_identity(
    program_id: &Pubkey,
    state_key: &Pubkey,
    state: &SeasonState,
) -> ProgramResult {
    if state.version != VERSION {
        return Err(ReceiptError::StateEncoding.into());
    }
    let (expected_pda, expected_bump) =
        Pubkey::find_program_address(&[SEASON_SEED, &state.season_id], program_id);
    if state_key != &expected_pda || state.bump != expected_bump {
        return Err(ReceiptError::InvalidSeasonPda.into());
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

fn validate_stored_season_state(
    program_id: &Pubkey,
    state_key: &Pubkey,
    state: &SeasonState,
) -> ProgramResult {
    state.assert_invariants()?;
    if state.compute_state_root()? != state.state_root {
        return Err(ReceiptError::StoredRootMismatch.into());
    }
    if state.seq == 0 {
        let expected_genesis_head = hashv(&[
            SEASON_GENESIS_DOMAIN,
            program_id.as_ref(),
            state_key.as_ref(),
            &state.season_id,
            &state.ruleset_hash,
            &state.payout_rules_hash,
            &state.state_root,
        ])
        .to_bytes();
        if state.head_event_hash != expected_genesis_head {
            return Err(ReceiptError::StoredRootMismatch.into());
        }
    } else if is_zero_hash(&state.head_event_hash) {
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

fn read_season_state(account: &AccountInfo) -> Result<SeasonState, ProgramError> {
    let data = account.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    SeasonState::deserialize(&mut slice).map_err(|_| ReceiptError::StateEncoding.into())
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

fn write_season_state(account: &AccountInfo, state: &SeasonState) -> ProgramResult {
    let bytes = borsh::to_vec(state).map_err(|_| ReceiptError::StateEncoding)?;
    if bytes.len() > account.data_len() {
        return Err(ProgramError::AccountDataTooSmall);
    }
    let mut data = account.try_borrow_mut_data()?;
    data.fill(0);
    data[..bytes.len()].copy_from_slice(&bytes);
    Ok(())
}

fn write_claim_receipt(account: &AccountInfo, receipt: &SeasonClaimReceipt) -> ProgramResult {
    let bytes = borsh::to_vec(receipt).map_err(|_| ReceiptError::StateEncoding)?;
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

    fn hex_value(value: &[u8]) -> String {
        value.iter().map(|byte| format!("{byte:02x}")).collect()
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

    fn season_fixture(payout_rules_hash: [u8; 32]) -> (Pubkey, Pubkey, SeasonState, Pubkey) {
        let program_id = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let season_id = bytes32(71);
        let (season_pda, bump) =
            Pubkey::find_program_address(&[SEASON_SEED, &season_id], &program_id);
        let args = InitializeSeasonArgs {
            season_id,
            ruleset_hash: bytes32(72),
            payout_rules_hash,
            expected_genesis_state_root: [0; 32],
        };
        let state = SeasonState::new(&program_id, &season_pda, bump, authority, &args).unwrap();
        (program_id, season_pda, state, authority)
    }

    fn credit_args(
        state: &SeasonState,
        source_kind: u8,
        gross_units: u64,
        event_byte: u8,
    ) -> CreditSeasonPurseArgs {
        CreditSeasonPurseArgs {
            source_kind,
            gross_units,
            expected_seq: state.seq,
            expected_head_event_hash: state.head_event_hash,
            event_id: bytes32(event_byte),
        }
    }

    fn finalize_args(
        state: &SeasonState,
        claim_root: [u8; 32],
        event_byte: u8,
    ) -> FinalizeSeasonArgs {
        FinalizeSeasonArgs {
            expected_seq: state.seq,
            expected_head_event_hash: state.head_event_hash,
            event_id: bytes32(event_byte),
            outcome_hash: bytes32(event_byte.wrapping_add(1)),
            chronicle_root: bytes32(event_byte.wrapping_add(2)),
            claim_root,
        }
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
        assert_eq!(genesis_bytes.len(), 365);
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
        let season_init = InitializeSeasonArgs {
            season_id: genesis.season_id,
            ruleset_hash: genesis.ruleset_hash,
            payout_rules_hash: bytes32(60),
            expected_genesis_state_root: bytes32(62),
        };
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::InitializeSeason(season_init)).unwrap()[0],
            5
        );
        let credit = CreditSeasonPurseArgs {
            source_kind: PURSE_SOURCE_ENTRY,
            gross_units: 10_000,
            expected_seq: 0,
            expected_head_event_hash: bytes32(63),
            event_id: bytes32(64),
        };
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::CreditSeasonPurse(credit)).unwrap()[0],
            6
        );
        let finalize = FinalizeSeasonArgs {
            expected_seq: 0,
            expected_head_event_hash: bytes32(65),
            event_id: bytes32(66),
            outcome_hash: bytes32(69),
            chronicle_root: bytes32(70),
            claim_root: bytes32(71),
        };
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::FinalizeSeason(finalize)).unwrap()[0],
            7
        );
        let claim = ClaimSeasonArgs {
            amount: 1,
            leaf_index: 0,
            merkle_proof: vec![],
            expected_seq: 0,
            expected_head_event_hash: bytes32(67),
            event_id: bytes32(68),
        };
        assert_eq!(
            borsh::to_vec(&PermutationInstruction::ClaimSeason(claim)).unwrap()[0],
            8
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
            ProgramError::Custom(ReceiptError::InvalidWorksiteState as u32)
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
    fn worksite_initialization_and_shape_guards_reject_ambiguous_civilizations() {
        let (program_id, state_pda, genesis, _, _, _) = fixture();
        let mut args = InitializeArgs {
            season_id: genesis.season_id,
            worksite_id: genesis.worksite_id,
            ruleset_hash: genesis.ruleset_hash,
            envoy: genesis.envoy,
            maker: genesis.maker,
            successor: genesis.successor,
            expected_genesis_state_root: genesis.state_root,
        };
        args.maker = args.envoy;
        assert_eq!(
            WorksiteState::new(
                &program_id,
                &state_pda,
                genesis.bump,
                genesis.authority,
                &args,
            )
            .unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidInitialization as u32)
        );

        let mut corrupted = genesis.clone();
        corrupted.seq = 2;
        corrupted.state_root = corrupted.compute_state_root().unwrap();
        assert_eq!(
            validate_stored_state(&corrupted).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidWorksiteState as u32)
        );

        let mut spoofed_season = genesis;
        spoofed_season.season_purse = Pubkey::new_unique();
        spoofed_season.state_root = spoofed_season.compute_state_root().unwrap();
        assert_eq!(
            validate_worksite_identity(&program_id, &state_pda, &spoofed_season).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidSeasonPda as u32)
        );
    }

    #[test]
    fn worksite_replay_chain_and_client_decision_hash_are_mandatory() {
        let (_, _, genesis, envoy, maker, _) = fixture();
        let args = args_for(&genesis, &envoy, EVENT_MARA_CHOICE, ACTION_OATH);

        let mut missing_decision = args.clone();
        missing_decision.client_event_hash = [0; 32];
        assert_eq!(
            genesis
                .preview_event(&envoy, &missing_decision)
                .unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidEventId as u32)
        );

        let after = genesis.preview_event(&envoy, &args).unwrap().0;
        assert_eq!(
            after.preview_event(&maker, &args).unwrap_err(),
            ProgramError::Custom(ReceiptError::StaleSequence as u32)
        );

        let mut wrong_previous_event = args.clone();
        wrong_previous_event.expected_prev_event_hash = bytes32(99);
        assert_eq!(
            genesis
                .preview_event(&envoy, &wrong_previous_event)
                .unwrap_err(),
            ProgramError::Custom(ReceiptError::PreviousEventMismatch as u32)
        );

        let mut wrong_prior_root = args;
        wrong_prior_root.prior_state_root = bytes32(98);
        assert_eq!(
            genesis
                .preview_event(&envoy, &wrong_prior_root)
                .unwrap_err(),
            ProgramError::Custom(ReceiptError::PriorRootMismatch as u32)
        );
    }

    #[test]
    fn constitution_splits_mock_units_exactly_and_preserves_every_unit() {
        let (_, _, season, authority) = season_fixture(bytes32(73));
        let entry_args = credit_args(&season, PURSE_SOURCE_ENTRY, 10_000, 80);
        let (after_entry, entry_receipt) = season.credit_purse(&authority, &entry_args).unwrap();
        assert_eq!(after_entry.entry_gross_units, 10_000);
        assert_eq!(after_entry.entry_purse_units, 7_000);
        assert_eq!(after_entry.ops_units, 3_000);
        assert_eq!(after_entry.purse_total, 7_000);
        assert_eq!(entry_receipt.previous_event_hash, season.head_event_hash);
        assert_eq!(entry_receipt.new_state_root, after_entry.state_root);

        let marketplace_args = credit_args(&after_entry, PURSE_SOURCE_MARKETPLACE, 100_000, 81);
        let (after_market, market_receipt) = after_entry
            .credit_purse(&authority, &marketplace_args)
            .unwrap();
        assert_eq!(after_market.marketplace_purse_units, 1_500);
        assert_eq!(after_market.seller_units, 97_500);
        assert_eq!(after_market.ops_units, 4_000);
        assert_eq!(after_market.purse_total, 8_500);
        assert_eq!(
            after_market.purse_total + after_market.ops_units + after_market.seller_units,
            110_000
        );
        assert_eq!(market_receipt.seq, 1);
        assert_eq!(
            market_receipt.previous_event_hash,
            after_entry.head_event_hash
        );
        assert_ne!(market_receipt.event_hash, entry_receipt.event_hash);
        after_market.assert_invariants().unwrap();

        let (_, _, tiny, tiny_authority) = season_fixture(bytes32(74));
        let tiny_entry = tiny
            .credit_purse(
                &tiny_authority,
                &credit_args(&tiny, PURSE_SOURCE_ENTRY, 1, 82),
            )
            .unwrap()
            .0;
        assert_eq!(tiny_entry.entry_purse_units, 0);
        assert_eq!(tiny_entry.ops_units, 1);
        let tiny_market = tiny_entry
            .credit_purse(
                &tiny_authority,
                &credit_args(&tiny_entry, PURSE_SOURCE_MARKETPLACE, 1, 83),
            )
            .unwrap()
            .0;
        assert_eq!(tiny_market.marketplace_purse_units, 0);
        assert_eq!(tiny_market.seller_units, 1);
        assert_eq!(tiny_market.ops_units, 1);
    }

    #[test]
    fn season_credit_rejects_wrong_actor_replays_bad_heads_and_boundaries() {
        let (_, _, season, authority) = season_fixture(bytes32(75));
        let stranger = Pubkey::new_unique();
        let valid = credit_args(&season, PURSE_SOURCE_ENTRY, 10_000, 84);
        assert_eq!(
            season.credit_purse(&stranger, &valid).unwrap_err(),
            ProgramError::Custom(ReceiptError::UnauthorizedLifecycleActor as u32)
        );

        let after = season.credit_purse(&authority, &valid).unwrap().0;
        assert_eq!(
            after.credit_purse(&authority, &valid).unwrap_err(),
            ProgramError::Custom(ReceiptError::StaleSeasonSequence as u32)
        );

        let mut bad_head = credit_args(&after, PURSE_SOURCE_ENTRY, 1, 85);
        bad_head.expected_head_event_hash = bytes32(10);
        assert_eq!(
            after.credit_purse(&authority, &bad_head).unwrap_err(),
            ProgramError::Custom(ReceiptError::SeasonHeadMismatch as u32)
        );

        let mut zero_event = credit_args(&after, PURSE_SOURCE_ENTRY, 1, 85);
        zero_event.event_id = [0; 32];
        assert_eq!(
            after.credit_purse(&authority, &zero_event).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidEventId as u32)
        );

        let bad_source = credit_args(&after, 99, 1, 86);
        assert_eq!(
            after.credit_purse(&authority, &bad_source).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidPurseSource as u32)
        );
        let zero = credit_args(&after, PURSE_SOURCE_ENTRY, 0, 87);
        assert_eq!(
            after.credit_purse(&authority, &zero).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidAmount as u32)
        );
        let too_large = credit_args(&after, PURSE_SOURCE_ENTRY, MAX_MOCK_CREDIT_UNITS + 1, 88);
        assert_eq!(
            after.credit_purse(&authority, &too_large).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidAmount as u32)
        );

        let mut seq_saturated = after;
        seq_saturated.seq = u64::MAX;
        let overflow = credit_args(&seq_saturated, PURSE_SOURCE_ENTRY, 1, 89);
        assert_eq!(
            seq_saturated
                .credit_purse(&authority, &overflow)
                .unwrap_err(),
            ProgramError::Custom(ReceiptError::PurseOverflow as u32)
        );
    }

    #[test]
    fn claims_are_impossible_until_finalization_and_are_merkle_and_head_bound() {
        let claimant = Pubkey::new_unique();
        let season_id = bytes32(71);
        let amount = 7_000;
        let claim_root = claim_leaf_hash(&season_id, &claimant, 0, amount);
        let (_, _, season, authority) = season_fixture(bytes32(76));
        assert_eq!(season.season_id, season_id);
        let registered = season
            .register_worksite(&authority, &Pubkey::new_unique())
            .unwrap()
            .0;
        assert_eq!(registered.active_worksites, 1);
        assert_eq!(registered.active_citizens, 3);
        let funded = registered
            .credit_purse(
                &authority,
                &credit_args(&registered, PURSE_SOURCE_ENTRY, 10_000, 90),
            )
            .unwrap()
            .0;

        let active_claim = ClaimSeasonArgs {
            amount,
            leaf_index: 0,
            merkle_proof: vec![],
            expected_seq: funded.seq,
            expected_head_event_hash: funded.head_event_hash,
            event_id: bytes32(91),
        };
        assert_eq!(
            funded.claim(&claimant, &active_claim).unwrap_err(),
            ProgramError::Custom(ReceiptError::SeasonNotFinalized as u32)
        );

        let finalized = funded
            .finalize(&authority, &finalize_args(&funded, claim_root, 92))
            .unwrap()
            .0;
        assert_eq!(finalized.status, SEASON_FINALIZED);
        assert_eq!(finalized.active_worksites, 0);
        assert_eq!(finalized.active_citizens, 0);
        assert_eq!(finalized.claimable_units, amount);
        assert_eq!(finalized.claim_root, claim_root);

        let mut invalid_proof = ClaimSeasonArgs {
            amount,
            leaf_index: 0,
            merkle_proof: vec![bytes32(55)],
            expected_seq: finalized.seq,
            expected_head_event_hash: finalized.head_event_hash,
            event_id: bytes32(95),
        };
        assert_eq!(
            finalized.claim(&claimant, &invalid_proof).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidMerkleProof as u32)
        );

        invalid_proof.merkle_proof.clear();
        let (claimed, receipt, leaf) = finalized.claim(&claimant, &invalid_proof).unwrap();
        assert_eq!(leaf, claim_root);
        assert_eq!(claimed.claimable_units, 0);
        assert_eq!(claimed.claimed_units, amount);
        assert_eq!(claimed.claim_count, 1);
        assert_eq!(receipt.event_kind, SEASON_EVENT_CLAIMED);
        assert_eq!(receipt.new_state_root, claimed.state_root);
        assert_eq!(
            claimed.claim(&claimant, &invalid_proof).unwrap_err(),
            ProgramError::Custom(ReceiptError::StaleSeasonSequence as u32)
        );
    }

    #[test]
    fn merkle_proof_direction_and_depth_are_canonical() {
        let season_id = bytes32(71);
        let left_actor = Pubkey::new_unique();
        let right_actor = Pubkey::new_unique();
        let left = claim_leaf_hash(&season_id, &left_actor, 0, 10);
        let right = claim_leaf_hash(&season_id, &right_actor, 1, 20);
        let root = hashv(&[CLAIM_NODE_DOMAIN, &left, &right]).to_bytes();
        assert!(verify_claim_proof(left, 0, &[right], &root));
        assert!(verify_claim_proof(right, 1, &[left], &root));
        assert!(!verify_claim_proof(left, 1, &[right], &root));
        assert!(!verify_claim_proof(left, 2, &[right], &root));
        assert!(!verify_claim_proof(
            left,
            0,
            &vec![bytes32(1); MAX_CLAIM_PROOF_DEPTH + 1],
            &root,
        ));
    }

    #[test]
    fn season_schema_fits_one_account_and_active_roots_cannot_be_forged() {
        let (program_id, season_pda, season, _) = season_fixture(bytes32(77));
        assert_eq!(borsh::to_vec(&season).unwrap().len(), 383);
        assert!(borsh::to_vec(&season).unwrap().len() <= SEASON_ACCOUNT_SPACE);

        let receipt = SeasonClaimReceipt {
            version: VERSION,
            bump: 1,
            season: season_pda,
            claimant: Pubkey::new_unique(),
            leaf_index: 0,
            amount: 1,
            leaf_hash: bytes32(78),
            season_event_hash: bytes32(79),
        };
        assert_eq!(borsh::to_vec(&receipt).unwrap().len(), 142);
        assert!(borsh::to_vec(&receipt).unwrap().len() <= CLAIM_ACCOUNT_SPACE);

        let mut forged = season.clone();
        forged.outcome_hash = bytes32(80);
        assert_eq!(
            forged.assert_invariants().unwrap_err(),
            ProgramError::Custom(ReceiptError::InvariantViolation as u32)
        );
        assert_eq!(
            validate_season_identity(&program_id, &Pubkey::new_unique(), &season).unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidSeasonPda as u32)
        );

        let registered = season
            .register_worksite(&season.authority, &Pubkey::new_unique())
            .unwrap()
            .0;
        let funded = registered
            .credit_purse(
                &season.authority,
                &credit_args(&registered, PURSE_SOURCE_ENTRY, 10_000, 96),
            )
            .unwrap()
            .0;
        let mut zero_root_finalize = finalize_args(&funded, bytes32(97), 98);
        zero_root_finalize.outcome_hash = [0; 32];
        assert_eq!(
            funded
                .finalize(&season.authority, &zero_root_finalize)
                .unwrap_err(),
            ProgramError::Custom(ReceiptError::InvalidFinalization as u32)
        );
    }

    #[test]
    fn cross_language_genesis_roots_are_stable() {
        let program_id = Pubkey::new_from_array([1; 32]);
        let authority = Pubkey::new_from_array([2; 32]);
        let season_id = [3; 32];
        let ruleset_hash = [4; 32];
        let payout_rules_hash = [5; 32];
        let (season_pda, season_bump) =
            Pubkey::find_program_address(&[SEASON_SEED, &season_id], &program_id);
        let season = SeasonState::new(
            &program_id,
            &season_pda,
            season_bump,
            authority,
            &InitializeSeasonArgs {
                season_id,
                ruleset_hash,
                payout_rules_hash,
                expected_genesis_state_root: [0; 32],
            },
        )
        .unwrap();
        let worksite_id = [6; 32];
        let (worksite_pda, worksite_bump) =
            Pubkey::find_program_address(&[WORKSITE_SEED, &season_id, &worksite_id], &program_id);
        let worksite = WorksiteState::new(
            &program_id,
            &worksite_pda,
            worksite_bump,
            authority,
            &InitializeArgs {
                season_id,
                worksite_id,
                ruleset_hash,
                envoy: Pubkey::new_from_array([7; 32]),
                maker: Pubkey::new_from_array([8; 32]),
                successor: Pubkey::new_from_array([9; 32]),
                expected_genesis_state_root: [0; 32],
            },
        )
        .unwrap();
        assert_eq!(
            season_pda.to_string(),
            "FBgcmTa8gv9wdkCoisSfFA3nNQayk8dPm2f3XCQRY34g"
        );
        assert_eq!(season_bump, 255);
        assert_eq!(
            hex_value(&season.state_root),
            "7fcca53cdf561370b3f823ec90c05522c451077aa59ae7a4966202ed2a6fb28d"
        );
        assert_eq!(
            hex_value(&season.head_event_hash),
            "405722c1ab97aefd6fd2fe4c105d0d2f6e2ccc2ed053bb88ef5de70bbafc5dc0"
        );
        assert_eq!(
            worksite_pda.to_string(),
            "BLsdrKQZVwkGjqS6XZH4evwmHg8j2rxiamYA2kB5C9uW"
        );
        assert_eq!(worksite_bump, 255);
        assert_eq!(
            hex_value(&worksite.state_root),
            "6a4974bca96781232b853abecd7c5448b3507c21cbd663da113a9f4edae3a968"
        );
        assert_eq!(
            hex_value(&worksite.head_event_hash),
            "3050fd23a4fcbc832e7901cf918453fb9eac9dae0c7d77c7d06ef483cb38879c"
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
