//! Reading the chain: log records of transactions and accounts of the season.

use borsh::BorshDeserialize;
use permutation_chain::state::{MemberAccount, RosterAccount, RosterEntry};
use permutation_server::chainlink::ChainLink;
use permutation_server::codec::{base64, base64_encode};
use serde_json::{json, Value};

/// Whether a log record's first field (its tag) is `tag`.
pub fn tagged(f: &[Vec<u8>], tag: &[u8]) -> bool {
    f.first().map(|t| t.as_slice()) == Some(tag)
}

/// A record's fields (`tag` first), re-read from the chain's transaction
/// logs, or `None` when the gateway indexed no transaction for it or the
/// transaction logged no such record.
pub fn fields(
    rpc: &ChainLink,
    sig: Option<&str>,
    tag: &[u8],
) -> Result<Option<Vec<Vec<u8>>>, String> {
    let Some(sig) = sig else {
        return Ok(None);
    };
    Ok(rpc.log_records(sig)?.into_iter().find(|f| tagged(f, tag)))
}

/// A tick input reassembled from its PS_INPUT records (the LogTickInput
/// transactions), if every chunk is there and the whole matches `hash`.
pub fn published_input(er: &ChainLink, sigs: Option<&Vec<Value>>, hash: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<Option<Vec<u8>>> = Vec::new();
    for sig in sigs?.iter().filter_map(|s| s.as_str()) {
        for f in er
            .log_records(sig)
            .ok()?
            .into_iter()
            .filter(|f| tagged(f, b"PS_INPUT"))
        {
            let (chunk, total) = (
                u16::from_le_bytes(f.get(2)?[..2].try_into().ok()?) as usize,
                u16::from_le_bytes(f.get(3)?[..2].try_into().ok()?) as usize,
            );
            if f.get(4)?.as_slice() != hash {
                continue;
            }
            chunks.resize(total, None);
            *chunks.get_mut(chunk)? = Some(f.get(5).cloned().unwrap_or_default());
        }
    }
    let bytes: Vec<u8> = chunks.into_iter().collect::<Option<Vec<_>>>()?.concat();
    (permutation_rules::hash::sha256(&[&bytes]).as_slice() == hash).then_some(bytes)
}

/// The season's roster account entries (operator AI members), if any.
pub fn roster_account(base: &ChainLink, program: &str, season_id: u64) -> Option<Vec<RosterEntry>> {
    let address = permutation_chain::state::roster_address(program, season_id)?;
    let data = base.account_data(&address).ok()??;
    let r = RosterAccount::deserialize(&mut &data[..]).ok()?;
    Some(r.entries)
}

/// Every Member account of the season on the base layer, in registration order.
pub fn season_members(base: &ChainLink, program: &str, season_id: u64) -> Vec<MemberAccount> {
    let id = base64_encode(&season_id.to_le_bytes());
    let magic = base64_encode(&permutation_chain::state::MEMBER_MAGIC);
    let res = base
        .rpc(
            "getProgramAccounts",
            json!([program, {"encoding": "base64", "commitment": "confirmed", "filters": [
                {"memcmp": {"offset": 0, "bytes": magic, "encoding": "base64"}},
                {"memcmp": {"offset": 8, "bytes": id, "encoding": "base64"}}]}]),
        )
        .unwrap_or(Value::Null);
    let mut out: Vec<MemberAccount> = res
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|a| base64(a["account"]["data"][0].as_str()?).ok())
        .filter_map(|d| MemberAccount::deserialize(&mut &d[..]).ok())
        .collect();
    out.sort_by_key(|m| m.index);
    out
}
