//! Transactions exactly in the relay's sponsored shapes (M1 contract §8.3,
//! §9.4): `[SetComputeUnitLimit(budget), SetComputeUnitPrice(0),
//! SetLoadedAccountsDataSizeLimit(L(kind)), one Frontier instruction]`,
//! fee payer = the relay key `GET /f/relay` drew, signed by every signer
//! except the fee payer (the relay co-signs). Budgets come from
//! `frontier-abi/vectors/budgets.json` (embedded at build time), the same
//! table the relay checks against.
//!
//! Direct transactions (personas only) are complete: the bot's own funded
//! key pays and signs.

use fclient::budgets::Budgets;
use fclient::tx::{self, TxBudget};
use fclient::{Address, Hash, Instruction, Keypair, Signer, Transaction};
use serde_json::Value;

use crate::ports::Answer;

/// The canonical budgets table (§10.2).
pub const BUDGETS_JSON: &str = include_str!("../../../../frontier-abi/vectors/budgets.json");

pub fn budgets() -> Budgets {
    let v: Value = serde_json::from_str(BUDGETS_JSON).expect("budgets.json");
    Budgets::from_json(&v).expect("budgets table")
}

/// `GET /f/relay`: the drawn fee payer and a blockhash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayInfo {
    pub fee_payer: Address,
    pub blockhash: Hash,
    pub last_valid_block_height: u64,
}

impl RelayInfo {
    pub fn from_answer(a: &Answer) -> Result<RelayInfo, String> {
        if !a.ok() {
            return Err(format!("GET /f/relay: {} {:?}", a.status, a.code()));
        }
        let s = |k: &str| {
            a.body
                .get(k)
                .and_then(|x| x.as_str())
                .ok_or_else(|| format!("GET /f/relay: no {k}"))
        };
        Ok(RelayInfo {
            fee_payer: s("feePayer")?
                .parse()
                .map_err(|_| "bad feePayer".to_string())?,
            blockhash: s("blockhash")?
                .parse()
                .map_err(|_| "bad blockhash".to_string())?,
            last_valid_block_height: a
                .body
                .get("lastValidBlockHeight")
                .and_then(|x| x.as_u64())
                .unwrap_or(0),
        })
    }
}

/// The sponsored shape of `ix` (tag = its first data byte), partially
/// signed by `signers` (never the fee payer).
pub fn sponsored(
    ix: Instruction,
    budgets: &Budgets,
    info: &RelayInfo,
    signers: &[&Keypair],
) -> Result<Transaction, String> {
    let tag = *ix.data.first().ok_or("empty instruction")?;
    let b = TxBudget::from_budget(budgets.get(tag), 0);
    let msg = tx::message(&[ix], &b, &info.fee_payer, &info.blockhash);
    let mut t = Transaction::new_unsigned(msg);
    if !signers.is_empty() {
        t.try_partial_sign(signers, Hash::new_from_array(info.blockhash.to_bytes()))
            .map_err(|e| e.to_string())?;
    }
    Ok(t)
}

/// A settle shape's requester signature: `requester` signs the message
/// bytes (§8.3 v1.3: the relay charges the requester's citizen only then).
pub fn requester_sig(t: &Transaction, requester: &Keypair) -> [u8; 64] {
    let sig = requester.sign_message(&t.message_data());
    let b: &[u8] = sig.as_ref();
    b.try_into().expect("64-byte signature")
}

/// A complete transaction paid by `payer` (a persona's own key) with the
/// kind's budget and a CU price (µlamports).
pub fn direct(
    ixs: &[Instruction],
    budgets: &Budgets,
    cu_price: u64,
    blockhash: &Hash,
    payer: &Keypair,
    others: &[&Keypair],
) -> Result<Transaction, String> {
    let tag = ixs
        .iter()
        .find(|i| i.program_id != fclient::addr::system_program())
        .and_then(|i| i.data.first().copied());
    let b = match tag {
        Some(t) => TxBudget::from_budget(budgets.get(t), cu_price),
        None => TxBudget {
            cu_limit: 20_000,
            cu_price,
            loaded_limit: fclient::fees::DEFAULT_LOADED_LIMIT,
            heap: None,
        },
    };
    let msg = tx::message(ixs, &b, &payer.pubkey(), blockhash);
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(others);
    tx::sign(msg, &signers)
}

pub fn wire_b64(t: &Transaction) -> String {
    frontier_agents::obs::b64_encode(&tx::wire(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fclient::abi::tag;
    use fclient::addr::Addresses;
    use fclient::ix::{self, HoldingRef, Player};

    #[test]
    fn a_sponsored_harvest_has_the_relays_shape() {
        let b = budgets();
        let a = Addresses::new(Address::new_from_array([0x42; 32]), 1);
        let wallet = Keypair::new_from_array([1; 32]);
        let session = Keypair::new_from_array([2; 32]);
        let fee_payer = Keypair::new_from_array([3; 32]);
        let info = RelayInfo {
            fee_payer: fee_payer.pubkey(),
            blockhash: Hash::new_from_array([9; 32]),
            last_valid_block_height: 10,
        };
        let p = Player {
            actor: session.pubkey(),
            payer: info.fee_payer,
            wallet: wallet.pubkey(),
        };
        let h = HoldingRef {
            p: 2,
            q: 0,
            site: 1,
        };
        let t = sponsored(ix::harvest(&a, &p, h), &b, &info, &[&session]).unwrap();
        // Four instructions: limit, price 0, loaded limit, the Frontier one.
        let pb = tx::parse_budget(&t.message);
        let want = b.get(tag::HARVEST);
        assert_eq!(pb.effective_cu_limit(), want.cu_limit);
        assert_eq!(pb.effective_loaded_limit(), want.loaded_limit);
        assert_eq!(pb.cu_price, Some(0), "CU price 0");
        assert_eq!(t.message.instructions.len(), 4);
        // The fee payer is the relay's and has not signed; the session has.
        assert_eq!(t.message.account_keys[0], info.fee_payer);
        assert_eq!(t.signatures[0], Default::default());
        let si = t
            .message
            .account_keys
            .iter()
            .position(|k| *k == session.pubkey())
            .unwrap();
        assert!(si < t.message.header.num_required_signatures as usize);
        let msg = t.message_data();
        assert!(t.signatures[si].verify(session.pubkey().as_ref(), &msg));
        // Round trip through the wire form the relay parses.
        let back = tx::from_wire(&tx::wire(&t)).unwrap();
        assert_eq!(back.message, t.message);
        // A requester signature verifies over the same message bytes.
        let rs = requester_sig(&t, &session);
        let sig = fclient::ports::Signature::from(rs);
        assert!(sig.verify(session.pubkey().as_ref(), &msg));
    }
}
