//! The lines the verifier prints (`✓ ✗ ? · – ⚠`, or JSON with `--json`) and
//! its verdict.

use serde_json::{json, Value};

/// What a line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// ✓ checked, and it holds.
    Pass,
    /// ✗ a contradiction.
    Fail,
    /// ? the chain data needed could not be read (nothing contradicts it).
    Incomplete,
    /// · a fact, not a claim.
    Info,
    /// – not checked (listed in the verdict).
    NotChecked,
    /// ⚠ a warning: holds, but depends on something named.
    Warn,
}

impl Status {
    pub fn mark(self) -> &'static str {
        match self {
            Status::Pass => "✓",
            Status::Fail => "✗",
            Status::Incomplete => "?",
            Status::Info => "·",
            Status::NotChecked => "–",
            Status::Warn => "⚠",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Incomplete => "incomplete",
            Status::Info => "info",
            Status::NotChecked => "not-checked",
            Status::Warn => "warn",
        }
    }
}

/// How far the season got (what the verdict can say).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Registering or Genesis: nothing to replay yet.
    NotStarted,
    /// Genesis done, the members are being seated.
    Seating,
    /// Running: the replay reached the live world at this tick (if it did).
    Running {
        tick: Option<u16>,
    },
    Finalized,
}

pub struct Line {
    pub what: String,
    pub status: Status,
    pub detail: String,
}

pub struct Report {
    pub lines: Vec<Line>,
    pub json: bool,
}

/// The verdict: its text, its JSON name and the exit code.
#[derive(Debug, PartialEq, Eq)]
pub struct Verdict {
    pub text: String,
    pub name: &'static str,
    pub code: i32,
}

impl Report {
    pub fn new(json: bool) -> Report {
        Report {
            lines: Vec::new(),
            json,
        }
    }

    /// Record a line, printing it at once unless the output is JSON.
    pub fn line(&mut self, status: Status, what: impl Into<String>, detail: impl Into<String>) {
        let (what, detail) = (what.into(), detail.into());
        if !self.json {
            println!(
                "{} {what}{}",
                status.mark(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {detail}")
                }
            );
        }
        self.lines.push(Line {
            what,
            status,
            detail,
        });
    }

    /// A check: ✓ if it held, ✗ if not.
    pub fn check(&mut self, what: impl Into<String>, ok: bool, detail: impl Into<String>) {
        self.line(if ok { Status::Pass } else { Status::Fail }, what, detail);
    }

    pub fn info(&mut self, what: impl Into<String>, detail: impl Into<String>) {
        self.line(Status::Info, what, detail);
    }

    /// Not checked: `what` is short (the verdict lists it), `why` says why.
    pub fn not_checked(&mut self, what: impl Into<String>, why: impl Into<String>) {
        self.line(Status::NotChecked, what, why);
    }

    pub fn incomplete(&mut self, what: impl Into<String>, why: impl Into<String>) {
        self.line(Status::Incomplete, what, why);
    }

    pub fn warn(&mut self, what: impl Into<String>, detail: impl Into<String>) {
        self.line(Status::Warn, what, detail);
    }

    fn count(&self, s: Status) -> usize {
        self.lines.iter().filter(|l| l.status == s).count()
    }

    pub fn failed(&self) -> usize {
        self.count(Status::Fail)
    }

    fn not_checked_list(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.status == Status::NotChecked)
            .map(|l| l.what.as_str())
            .collect()
    }

    /// The verdict: FAILED (exit 1) > INCOMPLETE (exit 3) > VERIFIED,
    /// VERIFIED SO FAR or NOT STARTED (exit 0).
    pub fn verdict(&self, stage: Stage) -> Verdict {
        let (failed, incomplete) = (self.failed(), self.count(Status::Incomplete));
        if failed > 0 {
            return Verdict {
                text: format!("FAILED: {failed} checks failed (✗ above)"),
                name: "failed",
                code: 1,
            };
        }
        if incomplete > 0 {
            return Verdict {
                text: "INCOMPLETE: the chain data needed to verify this season could not be read (? above); nothing read contradicts it.".into(),
                name: "incomplete",
                code: 3,
            };
        }
        let skipped = self.not_checked_list().join(", ");
        let (text, name) = match stage {
            Stage::NotStarted => ("NOT STARTED: nothing to replay yet".to_string(), "not-started"),
            Stage::Seating => (
                format!("VERIFIED SO FAR: genesis matches; the members are being seated. Not checked yet: {skipped}."),
                "verified-so-far",
            ),
            Stage::Running { tick } => (
                format!(
                    "VERIFIED SO FAR: the season is running; its replay reaches the live world{}. Not checked yet: {skipped}.",
                    tick.map(|t| format!(" at tick {t}")).unwrap_or_default()
                ),
                "verified-so-far",
            ),
            Stage::Finalized if skipped.is_empty() => (
                "VERIFIED: the season replays exactly as the chain recorded it.".to_string(),
                "verified",
            ),
            Stage::Finalized => (format!("VERIFIED (not checked: {skipped})."), "verified"),
        };
        Verdict {
            text,
            name,
            code: 0,
        }
    }

    /// Print the verdict (or the JSON report) and exit with its code.
    pub fn finish(&self, season_id: u64, stage: Stage, replayed: usize) -> ! {
        let v = self.verdict(stage);
        if self.json {
            let ok = |s: Status| !matches!(s, Status::Fail | Status::Incomplete);
            let out = json!({
                "season": season_id.to_string(),
                "ok": v.code == 0,
                "verdict": v.name,
                "complete": self.count(Status::NotChecked) == 0,
                "notChecked": self.not_checked_list(),
                "warnings": self.lines.iter().filter(|l| l.status == Status::Warn)
                    .map(|l| if l.detail.is_empty() { l.what.clone() } else { format!("{} — {}", l.what, l.detail) })
                    .collect::<Vec<_>>(),
                "ticks": replayed,
                "checks": self.lines.iter().map(|l| json!({
                    "check": l.what, "ok": ok(l.status), "status": l.status.name(), "detail": l.detail,
                })).collect::<Vec<Value>>(),
            });
            println!("{out}");
        } else {
            println!("{}", v.text);
        }
        std::process::exit(v.code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(lines: &[Status]) -> Report {
        let mut r = Report::new(true);
        for (i, s) in lines.iter().enumerate() {
            r.line(*s, format!("line {i}"), "");
        }
        r
    }

    #[test]
    fn report_statuses() {
        use Status::*;
        let r = report(&[Pass, Info, NotChecked, Warn]);
        let v = r.verdict(Stage::Finalized);
        assert_eq!((v.code, v.name), (0, "verified"));
        assert_eq!(v.text, "VERIFIED (not checked: line 2).");
        assert_eq!(r.not_checked_list(), vec!["line 2"]);
        let v = report(&[Pass]).verdict(Stage::Finalized);
        assert!(v.text.starts_with("VERIFIED: the season replays exactly"));
        let v = r.verdict(Stage::Running { tick: Some(4) });
        assert_eq!((v.code, v.name), (0, "verified-so-far"));
        assert!(v.text.starts_with("VERIFIED SO FAR"), "{}", v.text);
        assert!(v.text.contains("at tick 4") && v.text.contains("Not checked yet: line 2"));
        let v = report(&[Pass, Incomplete, Warn]).verdict(Stage::Finalized);
        assert_eq!((v.code, v.name), (3, "incomplete"));
        assert!(v.text.starts_with("INCOMPLETE"));
        let v = report(&[Incomplete, Fail, Pass]).verdict(Stage::Finalized);
        assert_eq!((v.code, v.name), (1, "failed"));
        assert!(v.text.starts_with("FAILED: 1 checks failed"));
        let v = report(&[NotChecked]).verdict(Stage::NotStarted);
        assert_eq!((v.code, v.name), (0, "not-started"));
    }
}
