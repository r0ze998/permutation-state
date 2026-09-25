//! Why an action is blocked, as `{ "code": …, …data }`.

use permutation_rules::checks::Blocked;
use serde_json::{json, Value};

use super::name;

pub fn blocked(b: Blocked) -> Value {
    use Blocked as B;
    let code = match b {
        B::NeedsTech(_) => "NeedsTech",
        B::TooCloseToCity { .. } => "TooCloseToCity",
        B::TooCloseToCityState { .. } => "TooCloseToCityState",
        B::NeedsPop { .. } => "NeedsPop",
        B::InTruce { .. } => "InTruce",
        B::BondTooSmall { .. } => "BondTooSmall",
        B::NotEnoughGold { .. } => "NotEnoughGold",
        B::AllianceFull { .. } => "AllianceFull",
        B::OutOfRange { .. } => "OutOfRange",
        B::OverCap { .. } => "OverCap",
        B::ProtectedCapital { .. } => "ProtectedCapital",
        B::OutOfBounds { .. } => "OutOfBounds",
        other => return json!({ "code": name(other) }),
    };
    let mut v = json!({ "code": code });
    let o = v.as_object_mut().unwrap();
    match b {
        B::NeedsTech(t) => {
            o.insert("tech".into(), json!(name(t)));
        }
        B::TooCloseToCity { distance, min } | B::TooCloseToCityState { distance, min } => {
            o.insert("distance".into(), json!(distance));
            o.insert("min".into(), json!(min));
        }
        B::NeedsPop { need, have } | B::NotEnoughGold { need, have } => {
            o.insert("need".into(), json!(need));
            o.insert("have".into(), json!(have));
        }
        B::InTruce { until } => {
            o.insert("until".into(), json!(until));
        }
        B::BondTooSmall { min } => {
            o.insert("min".into(), json!(min));
        }
        B::AllianceFull { cap } | B::OverCap { cap } => {
            o.insert("cap".into(), json!(cap));
        }
        B::OutOfBounds { min, max } => {
            o.insert("min".into(), json!(min));
            o.insert("max".into(), json!(max));
        }
        B::ProtectedCapital { civ, until } => {
            o.insert("civ".into(), json!(civ));
            o.insert(
                "until".into(),
                if until == u16::MAX {
                    Value::Null
                } else {
                    json!(until)
                },
            );
        }
        B::OutOfRange { distance, range } => {
            o.insert("distance".into(), json!(distance));
            o.insert("range".into(), json!(range));
        }
        _ => {}
    }
    v
}

pub(super) fn result<T>(r: Result<T, Blocked>) -> Value {
    match r {
        Ok(_) => Value::Null,
        Err(b) => blocked(b),
    }
}
