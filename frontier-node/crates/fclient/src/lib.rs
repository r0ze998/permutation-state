//! `fclient`: the Frontier off-chain spine (M1 contract §8.1).
//!
//! | module | what |
//! |---|---|
//! | [`abi`] | tags, classes, budgets, magics, sizes, errors, log kinds, layout offsets (re-exports `frontier-abi` from W2-F on) |
//! | [`addr`] | canonical with-seed and PDA addresses, citizen and keeper tags, host ids |
//! | [`ix`] | one builder per instruction, account order as §5 |
//! | [`decode`] | account decoders |
//! | [`log`] | the PS2 log parser, encoder and head chains |
//! | [`fees`], [`budgets`] | §10.1 formulas incl. `L(kind)`; per-kind CU and loaded-data limits |
//! | [`tx`] | compute budget, legacy messages, signing, shapes, priority |
//! | [`http`], [`rpc`] | loopback HTTP client, JSON-RPC client and `ChainPort` over it |
//! | [`ports`] | `ChainPort`, `DrandPort` and their types |
//! | [`beacon`] | quicknet info, blstrs verify, SP-V2 hints, seeds, the test-beacon key, drand backends |
//! | [`seal`] | the 37-B plaintext, tlock seals, the stock opener and seal codes |
//! | [`land`] | ticket scores and order, cohorts, expiry, the ring crowding rule, dormancy (W3-C) |
//! | [`play`] | seal opening, reveal order and slot index, path provinces, gather parts, settlement rank (W4-C) |
//! | [`clock`] | rule times and the `GameClock` |
//! | [`payers`] | reveal and delay pools, uniform draws, funders, payer care |
//! | [`vectors`] | `permutation-gateway/test/frontier-vectors.json` |
pub mod abi;
pub mod addr;
pub mod beacon;
pub mod budgets;
pub mod clock;
pub mod decode;
pub mod fees;
pub mod http;
pub mod ix;
pub mod land;
pub mod log;
pub mod payers;
pub mod play;
pub mod ports;
pub mod rpc;
pub mod seal;
pub mod tx;
pub mod vectors;

pub use solana_address::Address;
pub use solana_hash::Hash;
pub use solana_instruction::{AccountMeta, Instruction};
pub use solana_keypair::Keypair;
pub use solana_signer::Signer;
pub use solana_transaction::Transaction;
