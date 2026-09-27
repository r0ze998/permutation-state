//! Transactions: compute-budget instructions (limit, price, loaded-data
//! limit, heap frame), legacy messages, signing, wire form and shape
//! (bytes, locks, writes), and the reverse: reading a message's budget and
//! priority (the localnet block builder and the relay's shape checks use it).

use solana_address::Address;
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::Transaction;

use crate::addr;
use crate::budgets::Budget;
use crate::fees;

/// ComputeBudget instruction tags.
pub mod cb {
    pub const REQUEST_HEAP_FRAME: u8 = 1;
    pub const SET_COMPUTE_UNIT_LIMIT: u8 = 2;
    pub const SET_COMPUTE_UNIT_PRICE: u8 = 3;
    pub const SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT: u8 = 4;
}

pub fn set_compute_unit_limit(units: u32) -> Instruction {
    let mut d = vec![cb::SET_COMPUTE_UNIT_LIMIT];
    d.extend_from_slice(&units.to_le_bytes());
    Instruction {
        program_id: addr::compute_budget_program(),
        accounts: vec![],
        data: d,
    }
}
pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
    let mut d = vec![cb::SET_COMPUTE_UNIT_PRICE];
    d.extend_from_slice(&micro_lamports.to_le_bytes());
    Instruction {
        program_id: addr::compute_budget_program(),
        accounts: vec![],
        data: d,
    }
}
pub fn set_loaded_accounts_data_size_limit(bytes: u32) -> Instruction {
    let mut d = vec![cb::SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT];
    d.extend_from_slice(&bytes.to_le_bytes());
    Instruction {
        program_id: addr::compute_budget_program(),
        accounts: vec![],
        data: d,
    }
}
pub fn request_heap_frame(bytes: u32) -> Instruction {
    let mut d = vec![cb::REQUEST_HEAP_FRAME];
    d.extend_from_slice(&bytes.to_le_bytes());
    Instruction {
        program_id: addr::compute_budget_program(),
        accounts: vec![],
        data: d,
    }
}

/// System transfer (for payer care and airdrops in tests).
pub fn transfer(from: Address, to: Address, lamports: u64) -> Instruction {
    let mut d = vec![2, 0, 0, 0];
    d.extend_from_slice(&lamports.to_le_bytes());
    Instruction {
        program_id: addr::system_program(),
        accounts: vec![AccountMeta::new(from, true), AccountMeta::new(to, false)],
        data: d,
    }
}

/// What a transaction asks of the runtime besides its instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxBudget {
    pub cu_limit: u32,
    /// µlamports per CU (0 on every sponsored player shape, §8.3).
    pub cu_price: u64,
    /// `SetLoadedAccountsDataSizeLimit(L(kind))` on every transaction (I-45).
    pub loaded_limit: u32,
    /// `RequestHeapFrame` (only the keeper's retry ladder, I-50).
    pub heap: Option<u32>,
}

impl TxBudget {
    pub fn from_budget(b: Budget, cu_price: u64) -> TxBudget {
        TxBudget {
            cu_limit: b.cu_limit,
            cu_price,
            loaded_limit: b.loaded_limit,
            heap: None,
        }
    }

    /// The compute-budget prefix, in the order the relay's shape allowlist
    /// expects: limit, price, loaded-data limit (then the heap frame).
    pub fn instructions(&self) -> Vec<Instruction> {
        let mut v = vec![
            set_compute_unit_limit(self.cu_limit),
            set_compute_unit_price(self.cu_price),
            set_loaded_accounts_data_size_limit(self.loaded_limit),
        ];
        if let Some(h) = self.heap {
            v.push(request_heap_frame(h));
        }
        v
    }
}

/// `budget prefix ‖ ixs` as a legacy message paid by `payer`.
pub fn message(
    ixs: &[Instruction],
    budget: &TxBudget,
    payer: &Address,
    blockhash: &Hash,
) -> Message {
    let mut all = budget.instructions();
    all.extend_from_slice(ixs);
    Message::new_with_blockhash(&all, Some(payer), blockhash)
}

/// Signs `msg` with every signer (the fee payer first).
pub fn sign(msg: Message, signers: &[&Keypair]) -> Result<Transaction, String> {
    // Not `.clone()` (clippy's clone_on_copy fires when the workspace turns
    // on solana-hash's `copy` feature) and not a move (without it, Hash is
    // not Copy): rebuild it from its bytes.
    let bh = Hash::new_from_array(msg.recent_blockhash.to_bytes());
    let mut tx = Transaction::new_unsigned(msg);
    tx.try_sign(signers, bh).map_err(|e| e.to_string())?;
    Ok(tx)
}

/// Builds and signs in one step.
pub fn build(
    ixs: &[Instruction],
    budget: &TxBudget,
    signers: &[&Keypair],
    blockhash: &Hash,
) -> Result<Transaction, String> {
    let payer = signers.first().ok_or("no signer")?.pubkey();
    sign(message(ixs, budget, &payer, blockhash), signers)
}

/// Legacy wire bytes.
pub fn wire(tx: &Transaction) -> Vec<u8> {
    bincode::serialize(tx).expect("transaction serialises")
}

pub fn from_wire(b: &[u8]) -> Result<Transaction, String> {
    bincode::deserialize(b).map_err(|e| e.to_string())
}

/// The first signature (the transaction id).
pub fn signature(tx: &Transaction) -> Signature {
    tx.signatures.first().copied().unwrap_or_default()
}

/// Wire bytes, account locks and writable locks of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub bytes: usize,
    pub locks: usize,
    pub writes: usize,
    pub sigs: usize,
}

pub fn shape(msg: &Message) -> Shape {
    let n = msg.header.num_required_signatures as usize;
    let keys = msg.account_keys.len();
    let writes = (0..keys).filter(|&i| is_writable_index(msg, i)).count();
    let tx = Transaction {
        signatures: vec![Signature::default(); n],
        message: msg.clone(),
    };
    Shape {
        bytes: wire(&tx).len(),
        locks: keys,
        writes,
        sigs: n,
    }
}

/// Writable by the message header (before any reserved-key demotion).
pub fn is_writable_index(msg: &Message, i: usize) -> bool {
    let h = &msg.header;
    let (sig, ro_sig, ro_unsig) = (
        h.num_required_signatures as usize,
        h.num_readonly_signed_accounts as usize,
        h.num_readonly_unsigned_accounts as usize,
    );
    if i < sig {
        i < sig - ro_sig
    } else {
        i < msg.account_keys.len() - ro_unsig
    }
}

/// A message's compute budget as the runtime reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParsedBudget {
    pub cu_limit: Option<u32>,
    pub cu_price: Option<u64>,
    pub loaded_limit: Option<u32>,
    pub heap: Option<u32>,
    /// Instructions other than ComputeBudget ones.
    pub other_ixs: usize,
}

impl ParsedBudget {
    /// The limit the runtime applies: the requested one, else 200k per
    /// non-budget instruction, capped at 1.4M.
    pub fn effective_cu_limit(&self) -> u32 {
        self.cu_limit
            .unwrap_or((200_000 * self.other_ixs as u32).max(200_000))
            .min(crate::abi::CU_MAX)
    }
    pub fn effective_loaded_limit(&self) -> u32 {
        self.loaded_limit
            .unwrap_or(fees::RUNTIME_MAX_LOADED)
            .min(fees::RUNTIME_MAX_LOADED)
    }
}

pub fn parse_budget(msg: &Message) -> ParsedBudget {
    let cbp = addr::compute_budget_program();
    let mut p = ParsedBudget::default();
    for ci in &msg.instructions {
        let pid = msg.account_keys[ci.program_id_index as usize];
        if pid != cbp {
            p.other_ixs += 1;
            continue;
        }
        let d = &ci.data;
        let u32at = || {
            d.get(1..5)
                .map(|s| u32::from_le_bytes(s.try_into().expect("4")))
        };
        match d.first() {
            Some(&cb::SET_COMPUTE_UNIT_LIMIT) => p.cu_limit = u32at(),
            Some(&cb::SET_COMPUTE_UNIT_PRICE) => {
                p.cu_price = d
                    .get(1..9)
                    .map(|s| u64::from_le_bytes(s.try_into().expect("8")))
            }
            Some(&cb::SET_LOADED_ACCOUNTS_DATA_SIZE_LIMIT) => p.loaded_limit = u32at(),
            Some(&cb::REQUEST_HEAP_FRAME) => p.heap = u32at(),
            _ => {}
        }
    }
    p
}

/// A message's priority in milli-units (§10.1) and its cost.
pub fn priority(msg: &Message) -> (u64, u64) {
    let b = parse_budget(msg);
    let s = shape(msg);
    let limit = b.effective_cu_limit();
    let c = fees::cost(
        limit,
        s.sigs as u8,
        s.writes as u8,
        b.effective_loaded_limit(),
    );
    let fee = fees::priority_fee(b.cu_price.unwrap_or(0), limit);
    (fees::priority_milli(fee, c), c)
}

/// The fee the runtime charges: 5,000 per signature + the priority fee.
pub fn fee_lamports(msg: &Message) -> u64 {
    let b = parse_budget(msg);
    let sigs = msg.header.num_required_signatures as u64;
    sigs * fees::LAMPORTS_PER_SIGNATURE
        + fees::priority_fee(b.cu_price.unwrap_or(0), b.effective_cu_limit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_prefix_round_trips() {
        let kp = Keypair::new_from_array([3; 32]);
        let to = Address::new_from_array([4; 32]);
        let b = TxBudget {
            cu_limit: 26_000,
            cu_price: 1_000,
            loaded_limit: 1_048_576,
            heap: Some(262_144),
        };
        let tx = build(
            &[transfer(kp.pubkey(), to, 5)],
            &b,
            &[&kp],
            &Hash::new_from_array([1; 32]),
        )
        .unwrap();
        let p = parse_budget(&tx.message);
        assert_eq!(
            p,
            ParsedBudget {
                cu_limit: Some(26_000),
                cu_price: Some(1_000),
                loaded_limit: Some(1_048_576),
                heap: Some(262_144),
                other_ixs: 1
            }
        );
        let back = from_wire(&wire(&tx)).unwrap();
        assert_eq!(back, tx);
        let s = shape(&tx.message);
        assert_eq!((s.sigs, s.writes), (1, 2));
        // fee = 5,000 + ceil(1,000 × 26,000 / 10⁶) = 5,026.
        assert_eq!(fee_lamports(&tx.message), 5_026);
        let (p_milli, c) = priority(&tx.message);
        assert_eq!(c, 26_000 + 720 + 600 + 256);
        assert_eq!(p_milli, (26 + 2_500) * 1_000 / c);
    }
}
