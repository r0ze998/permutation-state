//! V10 — the report: `report.json` (the §8.5 schema, plus `checks_run`
//! and `provenance`) and `report.md` (the same, for people).

use serde_json::{json, Value};

use crate::{Finding, Report, Severity};

fn sev(s: Severity) -> &'static str {
    match s {
        Severity::Fail => "fail",
        Severity::Warn => "warn",
        Severity::Unverifiable => "unverifiable",
    }
}

fn finding_json(f: &Finding) -> Value {
    json!({
        "code": f.code, "severity": sev(f.severity), "check": f.check, "entity": f.entity,
        "bell": f.bell, "signature": f.signature, "detail": f.detail,
    })
}

pub fn json(r: &Report) -> Value {
    let c = &r.counts;
    let l = &r.liveness;
    json!({
        "verdict": r.verdict.name(),
        "season": r.season.to_string(),
        "program": r.program,
        "slot_range": [r.slot_range.0, r.slot_range.1],
        "entities": r.entities,
        "bells": r.bells,
        "counts": {"tx": c.tx, "failed_tx": c.failed_tx, "seals": c.seals, "reveals": c.reveals,
            "bad_seals": c.bad_seals, "clashes": c.clashes, "skips": c.skips, "tickets": c.tickets,
            "explores": c.explores},
        "findings": r.findings.iter().map(finding_json).collect::<Vec<_>>(),
        "liveness": {
            "valid_unrevealed": l.valid_unrevealed.iter().map(|(h, b, sigs)| json!({"host_id": h.to_string(), "arrive_bell": b, "attempts": sigs})).collect::<Vec<_>>(),
            "unrevealed_by_rule": l.unrevealed_by_rule.iter().map(|x| json!({"host_id": x.host.to_string(), "arrive_bell": x.arrive, "outcome": x.outcome, "reason": x.reason, "failed_attempts": x.failed_attempts})).collect::<Vec<_>>(),
            "unrevealed_by_reason": crate::checks::v5_seals::reason::ALL.iter().map(|&k| (k.to_string(), json!(l.unrevealed_by_rule.iter().filter(|x| x.reason == k).count()))).collect::<serde_json::Map<_, _>>(),
            "reveals_near_close": l.reveals_near_close,
            "max_anchor_delay_s": l.max_anchor_delay_s,
            "contested_bells": l.contested_bells.iter().map(|(p, q, b)| json!([p, q, b])).collect::<Vec<_>>(),
        },
        "checks_run": r.checks_run,
        "provenance": r.provenance,
        "mutated_build": crate::mutated(),
    })
}

pub fn markdown(r: &Report) -> String {
    let mut s = String::new();
    if let Some(m) = crate::mutated() {
        s.push_str(&format!(
            "> **MUTATED BUILD ({m}): a check is disabled; this is not a verification.**\n\n"
        ));
    }
    s.push_str(&format!(
        "# Frontier verifier report: **{}**\n\n",
        r.verdict.name()
    ));
    s.push_str(&format!(
        "- Season {} of program `{}`, slots {}–{}, {} bells, {} program accounts\n",
        r.season, r.program, r.slot_range.0, r.slot_range.1, r.bells, r.entities
    ));
    if !r.provenance.is_empty() {
        s.push_str(&format!("- Input: {}\n", r.provenance));
    }
    s.push_str(&format!("- Checks run: {}\n", r.checks_run.join(", ")));
    let c = &r.counts;
    s.push_str(&format!(
        "- Transactions {} ({} failed); seals {}, reveals {}, bad seals {}, clashes {}, skips {}, tickets {}, explores {}\n",
        c.tx, c.failed_tx, c.seals, c.reveals, c.bad_seals, c.clashes, c.skips, c.tickets, c.explores
    ));
    let l = &r.liveness;
    s.push_str(&format!(
        "- Liveness: {} valid seals unrevealed and routed ({} more unrevealed by rule: {}), {} reveals near the close, largest anchor delay {:.1} s, {} contested province-bells\n\n",
        l.valid_unrevealed.len(),
        l.unrevealed_by_rule.len(),
        crate::checks::v5_seals::reason::ALL
            .iter()
            .map(|&k| format!("{k} {}", l.unrevealed_by_rule.iter().filter(|x| x.reason == k).count()))
            .collect::<Vec<_>>()
            .join(", "),
        l.reveals_near_close,
        l.max_anchor_delay_s,
        l.contested_bells.len()
    ));
    if r.findings.is_empty() {
        s.push_str("No findings.\n");
        return s;
    }
    s.push_str("| Severity | Check | Code | Entity | Bell | Detail |\n|---|---|---|---|---|---|\n");
    for f in &r.findings {
        s.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} |\n",
            sev(f.severity),
            f.check,
            f.code,
            f.entity,
            f.bell,
            f.detail.replace('|', "\\|")
        ));
    }
    s
}
