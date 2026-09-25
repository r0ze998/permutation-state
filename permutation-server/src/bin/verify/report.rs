//! The checks the verifier prints (`✓`/`✗` lines, or JSON with `--json`).

use serde_json::{json, Value};

pub struct Report {
    /// Every check so far: what was checked, whether it held, and a detail.
    pub checks: Vec<(String, bool, String)>,
    pub json: bool,
}

impl Report {
    pub fn new(json: bool) -> Report {
        Report {
            checks: Vec::new(),
            json,
        }
    }

    /// Record a check, printing it at once unless the output is JSON.
    pub fn check(&mut self, what: impl Into<String>, ok: bool, detail: impl Into<String>) {
        let (what, detail) = (what.into(), detail.into());
        if !self.json {
            println!(
                "{} {what}{}",
                if ok { "✓" } else { "✗" },
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {detail}")
                }
            );
        }
        self.checks.push((what, ok, detail));
    }

    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|c| !c.1).count()
    }

    /// Print the verdict (or the JSON report) and exit: 0 if every check
    /// held, 1 otherwise.
    pub fn finish(&self, season_id: u64, replayed: usize) -> ! {
        let failed = self.failed();
        if self.json {
            let out = json!({
                "season": season_id.to_string(),
                "ok": failed == 0,
                "ticks": replayed,
                "checks": self.checks.iter().map(|(w, ok, d)| json!({"check": w, "ok": ok, "detail": d})).collect::<Vec<Value>>(),
            });
            println!("{out}");
        } else {
            println!(
                "{}",
                if failed == 0 {
                    "VERIFIED: the season replays exactly as the chain recorded it."
                } else {
                    "FAILED"
                }
            );
        }
        std::process::exit(if failed == 0 { 0 } else { 1 });
    }
}
