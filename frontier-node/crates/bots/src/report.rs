//! What the fleet did and what it saw, for the stack report (E5 criterion
//! 5: "every persona's expected outcome observed"; criterion 7: bot
//! outcome statistics, reported). Every sent action is one [`Outcome`];
//! the report counts them by group (archetype or persona), action and
//! result, and judges the personas whose expected outcome a bot can see
//! itself (a refusal it receives). The rest need the chain's final state
//! and are left `needs-chain` for the stack report and the verifier.

use std::collections::BTreeMap;

use frontier_agents::policy::SealKind;
use frontier_agents::{Arch, Persona};
use serde_json::{json, Value};

/// One action's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub bot: u32,
    pub arch: Arch,
    pub persona: Option<Persona>,
    /// The intent (`policy::Intent::name`).
    pub action: &'static str,
    /// `relay`, `keeper`, `direct`, `herald`, `local`.
    pub route: &'static str,
    pub ok: bool,
    /// HTTP status (0 for a direct transaction).
    pub status: u16,
    /// Refusal code (relay code name, keeper code, program error name).
    pub code: Option<String>,
    pub signature: Option<String>,
    /// Persona detail (e.g. `late`, `forged`, `zero_tip`).
    pub tag: Option<&'static str>,
}

impl Outcome {
    pub fn result(&self) -> String {
        if self.ok {
            "ok".into()
        } else {
            self.code
                .clone()
                .unwrap_or_else(|| format!("http_{}", self.status))
        }
    }
}

/// A persona's verdict from what its bots saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The expected refusal was seen and nothing contradicted it.
    Observed,
    /// Something the contract says must not happen happened (e.g. a late
    /// reveal accepted).
    Violated,
    /// The persona has not reached its test yet.
    Pending,
    /// Only the chain can tell (stack report, verifier).
    NeedsChain,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Observed => "observed",
            Verdict::Violated => "violated",
            Verdict::Pending => "pending",
            Verdict::NeedsChain => "needs-chain",
        }
    }
}

/// A march that settled with no REVEAL observed (W6T-3, w6-s7 R4: the
/// bots journalled `revealed` on the relay's 202 and never learnt that 27
/// honest Reveals were refused `Shielded`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unrevealed {
    pub bot: u32,
    pub persona: Option<Persona>,
    pub kind: SealKind,
    pub host: u64,
    pub depart_bell: u32,
    pub arrive: u32,
    pub dest: (i32, i32),
    /// How the owner revealed: `keeper` (`/f/reveal`), `direct`, or `none`.
    pub route: &'static str,
    /// The relay (or the chain) accepted the material.
    pub accepted: bool,
    pub tries: u8,
    /// The last refusal code of a Reveal attempt (e.g. `Shielded`).
    pub last_code: Option<String>,
}

impl Unrevealed {
    pub fn to_json(&self) -> Value {
        json!({
            "bot": self.bot,
            "persona": self.persona.map(|p| p.name()),
            "seal": match self.kind {
                SealKind::Honest => "honest",
                SealKind::Garbage => "garbage",
                SealKind::BadPlaintext => "bad_plaintext",
            },
            "host": self.host.to_string(),
            "depart_bell": self.depart_bell,
            "arrive": self.arrive,
            "dest": [self.dest.0, self.dest.1],
            "route": self.route,
            "accepted": self.accepted,
            "tries": self.tries,
            "last_code": self.last_code,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    /// (group, action, result) → count.
    pub counts: BTreeMap<(String, String, String), u64>,
    /// Outcomes of persona bots, kept whole (few: ≤ 1% of bots each).
    pub persona_outcomes: Vec<Outcome>,
    pub errors: BTreeMap<String, u64>,
    pub steps: u64,
    pub bots: u64,
    /// `/f/nudge` requests by result (`sent`, `failed`; integ-W4 review).
    pub nudges: BTreeMap<&'static str, u64>,
    /// Marches settled with no REVEAL observed (W6T-3).
    pub unrevealed: Vec<Unrevealed>,
}

fn refused_as(o: &Outcome, codes: &[&str]) -> bool {
    !o.ok && o.code.as_deref().is_some_and(|c| codes.contains(&c))
}

impl Report {
    pub fn record(&mut self, o: Outcome) {
        let group = match o.persona {
            Some(p) => format!("persona:{}", p.name()),
            None => format!("arch:{}", o.arch.key()),
        };
        *self
            .counts
            .entry((group, o.action.to_string(), o.result()))
            .or_default() += 1;
        if o.persona.is_some() {
            self.persona_outcomes.push(o);
        }
    }

    pub fn error(&mut self, what: &str) {
        *self.errors.entry(what.to_string()).or_default() += 1;
    }

    /// The verdict for `p` from its bots' outcomes (§8.6's brackets).
    pub fn verdict(&self, p: Persona) -> Verdict {
        let os: Vec<&Outcome> = self
            .persona_outcomes
            .iter()
            .filter(|o| o.persona == Some(p))
            .collect();
        let tagged =
            |t: &str| -> Vec<&&Outcome> { os.iter().filter(|o| o.tag == Some(t)).collect() };
        let judge = |xs: Vec<&&Outcome>, codes: &[&str]| {
            if xs.is_empty() {
                Verdict::Pending
            } else if xs.iter().any(|o| o.ok) {
                Verdict::Violated
            } else if xs.iter().all(|o| refused_as(o, codes)) {
                Verdict::Observed
            } else {
                // Refused, but not with the code the contract names: the
                // report lists the codes; the stack report decides.
                Verdict::NeedsChain
            }
        };
        match p {
            // integ-W6t review: the racer tries from its arrival bell on;
            // a try before the resolve (`NotResident`, `HostBusy`) tests
            // nothing and is left out.
            Persona::SettleRacer => judge(
                tagged("redepart")
                    .into_iter()
                    .filter(|o| !refused_as(o, &["NotResident", "HostBusy", "RateLimited"]))
                    .collect(),
                &["HostInTransit"],
            ),
            // integ-W6 (W6-C F1): the keeper's `202` on `/f/reveal` means
            // "queued" (§8.2, `/v1/reveal`: the keeper's pipeline never
            // sends at or after `A + W − 2 slots` and expires the track), not
            // a Reveal the program took, so it is not evidence either way.
            // A late Reveal the program accepts shows as the direct route's
            // `ok`, which still reads `violated`.
            // integ-W6t review: a late try that found the march already
            // revealed (`AlreadyDone`, w6-s7), settled (`TransitState`) or
            // its host gone on a new march (`CommitMismatch`, the 20× racer
            // check) never reached the window check; it is left out.
            Persona::LateRevealer => judge(
                tagged("late")
                    .into_iter()
                    .filter(|o| !(o.route == "keeper" && o.ok))
                    .filter(|o| !refused_as(o, &["AlreadyDone", "TransitState", "CommitMismatch"]))
                    .collect(),
                &["WindowClosed", "LatchClosed", "Archived", "TooLate"],
            ),
            // integ-W6t review: a forged SettleTransit that finds the
            // transit already settled (`TransitState`, `AlreadyDone`: a
            // keeper settled first) or not yet settleable (`TooEarly`)
            // never reached the account check; it
            // tests nothing either way, so it is left out (the rehearsal's
            // nightly 2: 7 forged Reveals refused `BadAddress`, 2 forged
            // settles `TransitState`, read `needs-chain`).
            Persona::Forger => judge(
                tagged("forged")
                    .into_iter()
                    .filter(|o| !refused_as(o, &["TransitState", "AlreadyDone", "TooEarly"]))
                    .collect(),
                &[
                    "BadAccount",
                    "BadAddress",
                    "WrongRegion",
                    "NoAnchor",
                    "RelayRejected",
                ],
            ),
            Persona::ZeroTip => judge(tagged("zero_tip"), &["TipTooLow", "TipNotPreset"]),
            Persona::Spammer => {
                let sp = tagged("spam");
                if sp.is_empty() {
                    Verdict::Pending
                } else if sp.iter().any(|o| {
                    o.status == 429 || refused_as(o, &["QuotaExceeded", "RateLimited", "Bucket"])
                }) {
                    Verdict::Observed
                } else {
                    Verdict::Pending
                }
            }
            _ => Verdict::NeedsChain,
        }
    }

    pub fn merge(&mut self, other: Report) {
        for (k, v) in other.counts {
            *self.counts.entry(k).or_default() += v;
        }
        self.persona_outcomes.extend(other.persona_outcomes);
        for (k, v) in other.errors {
            *self.errors.entry(k).or_default() += v;
        }
        self.steps += other.steps;
        self.bots += other.bots;
        self.unrevealed.extend(other.unrevealed);
    }

    pub fn to_json(&self) -> Value {
        let mut by_group: BTreeMap<&str, BTreeMap<String, BTreeMap<&str, u64>>> = BTreeMap::new();
        for ((g, a, r), n) in &self.counts {
            by_group
                .entry(g)
                .or_default()
                .entry(a.clone())
                .or_default()
                .insert(r, *n);
        }
        let personas: Vec<Value> = Persona::ALL
            .iter()
            .map(|&p| {
                let mut codes: BTreeMap<String, u64> = BTreeMap::new();
                for o in self
                    .persona_outcomes
                    .iter()
                    .filter(|o| o.persona == Some(p))
                {
                    // The route too (integ-W6, W6-C F1: a keeper `202`
                    // and a direct simulation read alike without it).
                    *codes
                        .entry(format!("{}@{}:{}", o.action, o.route, o.result()))
                        .or_default() += 1;
                }
                json!({
                    "persona": p.name(),
                    "expected": p.expected(),
                    "verdict": self.verdict(p).name(),
                    "locally_checkable": p.locally_checkable(),
                    "results": codes,
                })
            })
            .collect();
        json!({
            "v": 1,
            "bots": self.bots,
            "steps": self.steps,
            "groups": by_group,
            "personas": personas,
            "errors": self.errors,
            "nudges": self.nudges,
            "unrevealed": self.unrevealed.iter().map(Unrevealed::to_json).collect::<Vec<_>>(),
            "unrevealed_honest": self.unrevealed.iter().filter(|u| u.kind == SealKind::Honest).count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(p: Persona, tag: &'static str, ok: bool, code: &str, status: u16) -> Outcome {
        Outcome {
            bot: 1,
            arch: Arch::Bot,
            persona: Some(p),
            action: "x",
            route: "relay",
            ok,
            status,
            code: (!code.is_empty()).then(|| code.to_string()),
            signature: None,
            tag: Some(tag),
        }
    }

    /// integ-W6t review: a late Reveal refused because the march was
    /// already revealed or settled is the expected refusal.
    #[test]
    fn a_settle_racer_try_before_the_resolve_is_no_evidence() {
        let mut r = Report::default();
        r.record(o(Persona::SettleRacer, "redepart", false, "NotResident", 0));
        r.record(o(Persona::SettleRacer, "redepart", false, "HostBusy", 0));
        r.record(o(
            Persona::SettleRacer,
            "redepart",
            false,
            "RateLimited",
            429,
        ));
        assert_eq!(r.verdict(Persona::SettleRacer), Verdict::Pending);
        r.record(o(
            Persona::SettleRacer,
            "redepart",
            false,
            "HostInTransit",
            0,
        ));
        assert_eq!(r.verdict(Persona::SettleRacer), Verdict::Observed);
        r.record(o(Persona::SettleRacer, "redepart", true, "", 0));
        assert_eq!(r.verdict(Persona::SettleRacer), Verdict::Violated);
    }

    #[test]
    fn a_forged_settle_that_came_too_late_is_no_evidence() {
        let mut r = Report::default();
        r.record(o(Persona::Forger, "forged", false, "TransitState", 0));
        r.record(o(Persona::Forger, "forged", false, "TooEarly", 0));
        assert_eq!(r.verdict(Persona::Forger), Verdict::Pending);
        r.record(o(Persona::Forger, "forged", false, "BadAddress", 0));
        assert_eq!(r.verdict(Persona::Forger), Verdict::Observed);
        r.record(o(Persona::Forger, "forged", false, "Shielded", 0));
        assert_eq!(r.verdict(Persona::Forger), Verdict::NeedsChain);
        r.record(o(Persona::Forger, "forged", true, "", 0));
        assert_eq!(r.verdict(Persona::Forger), Verdict::Violated);
    }

    #[test]
    fn late_reveal_refused_after_reveal_or_settle_is_observed() {
        let mut r = Report::default();
        r.record(o(Persona::LateRevealer, "late", false, "TransitState", 0));
        r.record(o(Persona::LateRevealer, "late", false, "AlreadyDone", 0));
        r.record(o(Persona::LateRevealer, "late", false, "CommitMismatch", 0));
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Pending);
        r.record(o(Persona::LateRevealer, "late", false, "WindowClosed", 0));
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Observed);
        r.record(o(Persona::LateRevealer, "late", true, "", 0));
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Violated);
    }

    #[test]
    fn verdicts() {
        let mut r = Report::default();
        assert_eq!(r.verdict(Persona::ZeroTip), Verdict::Pending);
        r.record(o(Persona::ZeroTip, "zero_tip", false, "TipNotPreset", 400));
        r.record(o(Persona::ZeroTip, "zero_tip", false, "TipTooLow", 0));
        assert_eq!(r.verdict(Persona::ZeroTip), Verdict::Observed);
        r.record(o(Persona::LateRevealer, "late", false, "WindowClosed", 410));
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Observed);
        // A keeper's 202 is "queued", not a landing (W6-C F1).
        let mut queued = o(Persona::LateRevealer, "late", true, "", 202);
        queued.route = "keeper";
        r.record(queued);
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Observed);
        // A late Reveal the program took (the direct route) is a violation.
        let mut took = o(Persona::LateRevealer, "late", true, "", 0);
        took.route = "direct";
        r.record(took);
        assert_eq!(r.verdict(Persona::LateRevealer), Verdict::Violated);
        assert_eq!(
            r.to_json()["personas"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["persona"] == "late_revealer")
                .unwrap()["results"]["x@keeper:ok"],
            1
        );
        r.record(o(Persona::Spammer, "spam", true, "", 200));
        assert_eq!(r.verdict(Persona::Spammer), Verdict::Pending);
        r.record(o(Persona::Spammer, "spam", false, "QuotaExceeded", 429));
        assert_eq!(r.verdict(Persona::Spammer), Verdict::Observed);
        r.record(o(
            Persona::SettleRacer,
            "redepart",
            false,
            "NotResident",
            400,
        ));
        // integ-W6t review: a try before the resolve is no evidence.
        assert_eq!(r.verdict(Persona::SettleRacer), Verdict::Pending);
        r.record(o(Persona::SettleRacer, "redepart", false, "Shielded", 400));
        assert_eq!(r.verdict(Persona::SettleRacer), Verdict::NeedsChain);
        assert_eq!(r.verdict(Persona::MinTip), Verdict::NeedsChain);
        let j = r.to_json();
        assert_eq!(j["personas"].as_array().unwrap().len(), 13);
    }
}
