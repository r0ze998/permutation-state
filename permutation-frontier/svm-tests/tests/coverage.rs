//! G13's guard (§3.5, §13.3): the (instruction, error code) registry in
//! `src/cover/` names real, non-ignored tests whose bodies assert what the
//! registry says. With `RELEASE_CHECK=1` nothing may be `Pending` and every
//! program code (except the exempt ones) must be asserted by some test.

use std::collections::BTreeSet;
use std::path::Path;

use permutation_frontier_svm_tests::cover::{self, covered_by, Code, Cover, EXEMPT, OPT_IN};
use permutation_frontier_svm_tests::{FrontierError, Ix};

fn release_check() -> bool {
    std::env::var("RELEASE_CHECK").is_ok_and(|v| v == "1")
}

fn strip_comments(s: &str) -> String {
    s.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body of `fn name` in `tests/<file>.rs` and whether it is ignored.
fn test_body(file: &str, name: &str) -> Result<(String, bool), String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(format!("{file}.rs"));
    let src = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let at = src
        .find(&format!("fn {name}("))
        .ok_or_else(|| format!("tests/{file}.rs has no fn {name}"))?;
    let head = &src[..at];
    let attrs_from = head.rfind("\n}").map_or(0, |i| i + 2);
    let attrs = &head[attrs_from..];
    if !attrs.contains("#[test]") {
        return Err(format!("{file}::{name} is not a #[test]"));
    }
    let ignored = attrs.contains("#[ignore");
    let open = src[at..].find('{').ok_or("no body")? + at;
    let mut depth = 0usize;
    let mut end = open;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    Ok((strip_comments(&src[open..=end]), ignored))
}

#[test]
fn g13_coverage_registry_names_real_tests_that_assert_their_codes() {
    let mut problems = vec![];
    let mut asserted: BTreeSet<u32> = BTreeSet::new();
    let mut pending = vec![];
    for ix in Ix::ALL {
        for cov in covered_by(*ix) {
            match cov {
                Cover::Pending(unit) => pending.push(format!("{} → {unit}", ix.name())),
                Cover::Test(name, codes) => {
                    let (file, f) = name.split_once("::").expect("file::function");
                    let (body, ignored) = match test_body(file, f) {
                        Ok(x) => x,
                        Err(e) => {
                            problems.push(format!("{}: {e}", ix.name()));
                            continue;
                        }
                    };
                    let opt_in = OPT_IN.iter().any(|(t, _)| *t == *name);
                    if ignored && !opt_in {
                        problems.push(format!("{}: {name} is ignored", ix.name()));
                    }
                    for c in *codes {
                        let (needles, what): (Vec<String>, String) = match c {
                            Code::Err(e) => {
                                asserted.insert(e.code());
                                (vec![format!("E::{e:?}")], format!("E::{e:?}"))
                            }
                            Code::Lands(n) => (
                                vec![n.to_string(), "expect_lands(".into()],
                                format!("lands {n}"),
                            ),
                            Code::Refused(n) => (
                                vec![n.to_string(), "assert_refused(".into()],
                                format!("refused {n}"),
                            ),
                            Code::Loaded => (vec!["check(".into()], "loaded".into()),
                        };
                        for n in needles {
                            if !body.contains(&n) {
                                problems.push(format!(
                                    "{}: {name} does not assert {what} (no `{n}`)",
                                    ix.name()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "coverage registry:\n{}",
        problems.join("\n")
    );
    println!("{} Pending entries:\n{}", pending.len(), pending.join("\n"));
    if release_check() {
        assert!(
            pending.is_empty(),
            "RELEASE_CHECK=1: pending coverage:\n{}",
            pending.join("\n")
        );
        let missing: Vec<_> = FrontierError::ALL
            .iter()
            .filter(|e| !EXEMPT.iter().any(|(x, _)| x == *e))
            .filter(|e| !asserted.contains(&e.code()))
            .map(|e| e.name())
            .collect();
        assert!(
            missing.is_empty(),
            "RELEASE_CHECK=1: codes no test asserts: {missing:?}"
        );
        assert!(
            cover::pending().is_empty(),
            "RELEASE_CHECK=1: ignored tests pending a fix"
        );
    }
}

#[test]
fn g13_coverage_exempt_codes_are_reserved_ones() {
    for (e, why) in EXEMPT {
        assert!(
            matches!(
                e,
                FrontierError::Reserved14
                    | FrontierError::TipNotPreset
                    | FrontierError::NotImplemented
            ),
            "{e:?} exempt: {why}"
        );
    }
}
