//! Per-kind transaction budgets: the CU limit to request and the
//! loaded-data limit `L(kind)` (§10.2, I-45, I-50).
//!
//! The generated table lives in `frontier-abi::budgets` (W1-E) and its
//! `vectors/budgets.json`; until the release `.so` is measured every kind
//! requests its §5.5 CU budget and `L` = 1 MiB (the contract's working
//! default). [`Budgets::from_json`] loads a table in the shape below so the
//! keeper and the relay follow the generated file as soon as it exists:
//!
//! ```json
//! {"budgets": [{"tag": 81, "cu_limit": 26000, "loaded_limit": 1048576}, ...]}
//! ```

use std::collections::BTreeMap;

use crate::abi::{self, INSTRUCTIONS};
use crate::fees::DEFAULT_LOADED_LIMIT;

/// What a transaction of one instruction kind requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub cu_limit: u32,
    pub loaded_limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budgets {
    by_tag: BTreeMap<u8, Budget>,
}

impl Default for Budgets {
    fn default() -> Self {
        Budgets::placeholder()
    }
}

impl Budgets {
    /// §5.5 CU budgets and `L` = 1 MiB for every kind (wave 1).
    pub fn placeholder() -> Budgets {
        let by_tag = INSTRUCTIONS
            .iter()
            .map(|i| {
                (
                    i.tag,
                    Budget {
                        cu_limit: i.cu_budget.min(abi::CU_MAX),
                        loaded_limit: DEFAULT_LOADED_LIMIT,
                    },
                )
            })
            .collect();
        Budgets { by_tag }
    }

    pub fn get(&self, tag: u8) -> Budget {
        self.by_tag.get(&tag).copied().unwrap_or(Budget {
            cu_limit: abi::CU_MAX,
            loaded_limit: DEFAULT_LOADED_LIMIT,
        })
    }

    pub fn set(&mut self, tag: u8, b: Budget) {
        self.by_tag.insert(tag, b);
    }

    /// Loads rows `{tag, cu_limit, loaded_limit}` over the placeholder:
    /// the canonical `frontier-abi/vectors/budgets.json` (rows under
    /// `instructions`, §10.2) or this type's own `to_json` (`budgets`).
    pub fn from_json(v: &serde_json::Value) -> Result<Budgets, String> {
        let mut out = Budgets::placeholder();
        let rows = v
            .get("instructions")
            .or_else(|| v.get("budgets"))
            .and_then(|b| b.as_array())
            .ok_or("missing `instructions` (or `budgets`) array")?;
        for r in rows {
            let tag = r
                .get("tag")
                .and_then(|x| x.as_u64())
                .ok_or("row without tag")? as u8;
            let cu = r
                .get("cu_limit")
                .and_then(|x| x.as_u64())
                .ok_or("row without cu_limit")? as u32;
            let l = r
                .get("loaded_limit")
                .and_then(|x| x.as_u64())
                .ok_or("row without loaded_limit")? as u32;
            if !l.is_multiple_of(crate::fees::PAGE) {
                return Err(format!(
                    "tag {tag:#x}: loaded_limit {l} is not a multiple of 32 KiB"
                ));
            }
            out.set(
                tag,
                Budget {
                    cu_limit: cu.min(abi::CU_MAX),
                    loaded_limit: l,
                },
            );
        }
        Ok(out)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "budgets": self.by_tag.iter().map(|(t, b)| serde_json::json!({
                "tag": t, "cu_limit": b.cu_limit, "loaded_limit": b.loaded_limit
            })).collect::<Vec<_>>()
        })
    }

    /// The keeper's retry ladder (I-50): `ComputeBudgetExceeded` →
    /// `min(2 × limit, 1.4M)`, then 1.4M.
    pub fn retry_cu(limit: u32) -> u32 {
        limit.saturating_mul(2).min(abi::CU_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_and_json() {
        let b = Budgets::placeholder();
        assert_eq!(
            b.get(abi::tag::REVEAL),
            Budget {
                cu_limit: 26_000,
                loaded_limit: 1_048_576
            }
        );
        let j = serde_json::json!({"budgets": [{"tag": 81, "cu_limit": 21000, "loaded_limit": 720896}]});
        let b2 = Budgets::from_json(&j).unwrap();
        assert_eq!(b2.get(81).cu_limit, 21_000);
        assert_eq!(Budgets::from_json(&b2.to_json()).unwrap(), b2);
        let bad =
            serde_json::json!({"budgets": [{"tag": 81, "cu_limit": 1, "loaded_limit": 1000}]});
        assert!(Budgets::from_json(&bad).is_err());
        assert_eq!(Budgets::retry_cu(26_000), 52_000);
        assert_eq!(Budgets::retry_cu(900_000), 1_400_000);
    }

    /// §10.2: the canonical budgets file loads (W2-F consumes it).
    #[test]
    fn loads_frontier_abis_budgets_json() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../frontier-abi/vectors/budgets.json");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("budgets.json"))
                .expect("json");
        let b = Budgets::from_json(&v).expect("loads");
        let reveal = b.get(crate::abi::tag::REVEAL);
        assert_eq!(reveal.cu_limit, 26_000);
        assert_eq!(reveal.loaded_limit % crate::fees::PAGE, 0);
        let again = Budgets::from_json(&b.to_json()).expect("round trip");
        assert_eq!(again.get(crate::abi::tag::REVEAL), reveal);
    }
}
