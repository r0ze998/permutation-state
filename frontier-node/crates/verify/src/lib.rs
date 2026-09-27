//! `verify-core` (M1 contract §8.5).
//!
//! **W1 skeleton.** W4 builds checks V1–V13 and tampers T1–T22. What exists
//! now is the report schema and the exit codes, which the stack and the
//! tamper suite depend on. V1's chain walk is `fclient::log::Chains`.

use serde_json::{json, Value};

/// Exit codes: 0 PASS, 1 FAIL, 2 cannot verify.
pub const EXIT_PASS: i32 = 0;
pub const EXIT_FAIL: i32 = 1;
pub const EXIT_UNVERIFIABLE: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Unverifiable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub fail: bool,
    pub entity: String,
    pub bell: u32,
    pub signature: String,
    pub detail: String,
}

/// `report.json` (§8.5 schema).
pub fn report(verdict: Verdict, season: &str, program: &str, findings: &[Finding]) -> Value {
    json!({
        "verdict": match verdict { Verdict::Pass => "PASS", Verdict::Fail => "FAIL", Verdict::Unverifiable => "UNVERIFIABLE" },
        "season": season, "program": program, "slot_range": [0, 0], "entities": 0, "bells": 0,
        "counts": {"tx": 0, "failed_tx": 0, "seals": 0, "reveals": 0, "bad_seals": 0, "clashes": 0, "skips": 0, "tickets": 0, "explores": 0},
        "findings": findings.iter().map(|f| json!({"code": f.code, "severity": if f.fail { "fail" } else { "warn" },
            "entity": f.entity, "bell": f.bell, "signature": f.signature, "detail": f.detail})).collect::<Vec<_>>(),
        "liveness": {"valid_unrevealed": [], "reveals_near_close": 0, "max_anchor_delay_s": 0.0, "contested_bells": []},
    })
}

pub fn exit_code(v: Verdict) -> i32 {
    match v {
        Verdict::Pass => EXIT_PASS,
        Verdict::Fail => EXIT_FAIL,
        Verdict::Unverifiable => EXIT_UNVERIFIABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_schema() {
        let f = Finding {
            code: "HeadMismatch".into(),
            fail: true,
            entity: "x".into(),
            bell: 3,
            signature: "s".into(),
            detail: "d".into(),
        };
        let r = report(Verdict::Fail, "1", "p", &[f]);
        assert_eq!(r["verdict"], "FAIL");
        assert_eq!(r["findings"][0]["severity"], "fail");
        assert_eq!(exit_code(Verdict::Unverifiable), 2);
    }
}
