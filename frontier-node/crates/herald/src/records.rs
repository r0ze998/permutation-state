//! PS2 records and account keys as the herald names them (contract §6,
//! §8.4, §9.2).
//!
//! - [`record_json`]: a record decoded field by field from the
//!   `frontier-abi::log` kind table (`/h/events`; the raw body travels
//!   beside it, the decoded form is a display convenience).
//! - [`account_key`]: the key string of an account (`pv:<P>,<Q>`,
//!   `ar:<P>,<Q>,<b>,<f>,<i>`, `ad:<P>,<Q>,<day>`, `ci:<P>,<Q>,<b>` as §9.2,
//!   and the same grammar for the other kinds) used in envelopes and WS
//!   messages.
//! - [`province_of_record`]: the province a record is about, for WS routing.

use serde_json::{json, Map, Value};

use fclient::decode::Bytes;
use frontier_abi::addr::split_host_id;
use frontier_abi::layout::AccountKind;
use frontier_abi::log::{self as plog, Kind};

pub const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Fields decoded as signed integers.
const SIGNED: &[&str] = &[
    "p",
    "q",
    "origin_p",
    "origin_q",
    "t_create_min",
    "genesis_ts",
    "t_open",
    "a",
    "from_ts",
    "expiry",
    "final_ts",
    "done_at",
    "delta",
    "pool_owed_delta",
];

fn field_value(name: &str, b: &[u8]) -> Value {
    let signed = SIGNED.contains(&name);
    match b.len() {
        1 => json!(b[0]),
        2 if signed => json!(i16::from_le_bytes([b[0], b[1]])),
        2 => json!(u16::from_le_bytes([b[0], b[1]])),
        4 if signed => json!(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        4 => json!(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        8 => {
            let a: [u8; 8] = b.try_into().unwrap_or_default();
            if signed {
                json!(i64::from_le_bytes(a).to_string())
            } else {
                json!(u64::from_le_bytes(a).to_string())
            }
        }
        _ => json!(hex::encode(b)),
    }
}

fn fields(spec: &[(&str, usize)], b: &[u8]) -> Value {
    let mut m = Map::new();
    let mut o = 0;
    for (n, w) in spec {
        if let Some(s) = b.get(o..o + w) {
            m.insert((*n).into(), field_value(n, s));
        }
        o += w;
    }
    Value::Object(m)
}

/// A PS2 body decoded: `{kind, name, bell, key: {…}, payload: {…}, links:
/// [{entity, seq, head}]}`; `None` if the body does not decode.
pub fn record_json(body: &[u8]) -> Option<Value> {
    let r = plog::decode(body).ok()?;
    let spec = r.kind.spec();
    let links: Vec<Value> = r
        .links
        .iter()
        .take(r.n_links)
        .flatten()
        .map(|l| json!({"entity": l.entity as u8, "seq": l.seq.to_string(), "head": hex::encode(l.head)}))
        .collect();
    Some(json!({
        "kind": r.kind as u8, "name": spec.name, "bell": r.bell,
        "key": fields(spec.key, r.key), "payload": fields(spec.payload, r.payload), "links": links,
    }))
}

fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// The province a record is about: its `p`, `q` key fields, or the
/// province of the host its `host_id` key names.
pub fn province_of_record(kind: Kind, key: &[u8]) -> Option<(i32, i32)> {
    let spec = kind.spec();
    match spec.key.first().map(|f| f.0) {
        Some("p") => Some((i32_at(key, 0)?, i32_at(key, 4)?)),
        Some("host_id") => {
            let h = split_host_id(u64_at(key, 0)?)?;
            Some((h.province.p, h.province.q))
        }
        _ => None,
    }
}

/// The `(bell, region)` of a beacon record (ANCHOR, SEED).
pub fn bell_region_of_record(kind: Kind, key: &[u8]) -> Option<(u32, u8)> {
    match kind {
        Kind::ANCHOR | Kind::SEED => Some((u32_at(key, 0)?, *key.get(4)?)),
        _ => None,
    }
}

/// A short key of a record for WS messages: `NAME:field,field,…` over the
/// key's integer fields.
pub fn record_key(kind: Kind, key: &[u8]) -> String {
    let spec = kind.spec();
    let mut parts = vec![];
    let mut o = 0;
    for (n, w) in spec.key {
        if let Some(s) = key.get(o..o + w) {
            parts.push(match field_value(n, s) {
                Value::String(s) => s,
                v => v.to_string(),
            });
        }
        o += w;
    }
    format!("{}:{}", spec.name, parts.join(","))
}

/// The herald key of a program account from its bytes (kind by magic).
pub fn account_key(kind: AccountKind, d: &[u8], address: &str) -> String {
    let b = Bytes(d);
    let g = |o: usize, n: usize| d.len() >= o + n;
    use AccountKind::*;
    match kind {
        Province if g(64, 4) => format!("pv:{},{}", b.i16(64), b.i16(66)),
        ArrivalSlot if g(16, 10) => format!(
            "ar:{},{},{},{},{}",
            b.i16(16),
            b.i16(18),
            b.u32(20),
            b.u8(24),
            b.u8(25)
        ),
        ArrivalDay if g(16, 8) => format!("ad:{},{},{}", b.i16(16), b.i16(18), b.u32(20)),
        ClashInputs if g(64, 8) => format!("ci:{},{},{}", b.i16(64), b.i16(66), b.u32(68)),
        Holding if g(64, 5) => format!("ho:{},{},{}", b.i16(64), b.i16(66), b.u8(68)),
        BellAnchor if g(16, 5) => format!("an:{},{}", b.u32(16), b.u8(20)),
        SeedCache if g(16, 6) => format!("sd:{},{},{}", b.u32(16), b.u8(20), b.u8(21)),
        AnchorArchive if g(16, 8) => format!("aa:{},{}", b.u8(16), b.u32(20)),
        BeaconLog if g(16, 1) => format!("bl:{}", b.u8(16)),
        RingSeed if g(16, 2) => format!("rs:{}", b.u16(16)),
        ProvinceFund if g(16, 1) => format!("pf:{}", b.u8(16)),
        JoinShard if g(64, 2) => format!("js:{},{}", b.u8(64), b.u8(65)),
        Season => format!("se:{}", b.u64(8)),
        Frontier => "fr".into(),
        DefencePool => "dp".into(),
        Citizen => format!("ct:{address}"),
        DefenceClaim => format!("dc:{address}"),
        _ => format!("?:{address}"),
    }
}

/// `(bell, region)` of an account the bell-region records are made of.
pub fn bell_region_of_account(kind: AccountKind, d: &[u8]) -> Option<(u32, u8)> {
    match kind {
        AccountKind::BellAnchor | AccountKind::SeedCache => Some((u32_at(d, 16)?, *d.get(20)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frontier_abi::log::{write_body, write_tail};

    #[test]
    fn decodes_records_and_names_their_province() {
        let key: Vec<u8> = [
            2i32.to_le_bytes(),
            (-1i32).to_le_bytes(),
            40u32.to_le_bytes(),
        ]
        .concat();
        let mut payload = vec![7u8; 32];
        payload.extend([8u8; 32]);
        payload.extend(3u32.to_le_bytes());
        payload.extend([0u8; 9]);
        let mut out = vec![0u8; 256];
        let n = write_body(Kind::CLASH, 40, &key, &payload, &mut out).unwrap();
        let m = write_tail(&[], &mut out, n).unwrap();
        let v = record_json(&out[..m]).unwrap();
        assert_eq!(v["name"], "CLASH");
        assert_eq!(v["key"]["p"], 2);
        assert_eq!(v["key"]["q"], -1);
        assert_eq!(v["key"]["bell"], 40);
        assert_eq!(v["payload"]["engagements"], 3);
        assert_eq!(v["payload"]["outcome_digest"], hex::encode([7u8; 32]));
        assert_eq!(province_of_record(Kind::CLASH, &key), Some((2, -1)));
        assert_eq!(record_key(Kind::CLASH, &key), "CLASH:2,-1,40");
        let hid = frontier_abi::addr::host_id(3, -2, 1, 0, 9).unwrap();
        assert_eq!(
            province_of_record(Kind::DEPART, &hid.to_le_bytes()),
            Some((3, -2))
        );
        assert_eq!(province_of_record(Kind::ANCHOR, &[0; 5]), None);
        assert_eq!(
            bell_region_of_record(Kind::ANCHOR, &[40, 0, 0, 0, 13]),
            Some((40, 13))
        );
    }
}
