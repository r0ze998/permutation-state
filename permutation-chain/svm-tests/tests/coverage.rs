//! The coverage guard: every instruction has a positive and a negative
//! test, every error code is asserted somewhere, every ignored test is on a
//! ledger (`src/cover`), signer checks run with sigverify on, and the play
//! server never builds the program feature.
//!
//! The test index is built from the files in `tests/` with comments
//! stripped, so a name, an `E::X` or a builder that appears only in a
//! comment, or only in an ignored test, counts for nothing.
//! `RELEASE_CHECK=1` (CI on tags and `release/*`) also requires that no
//! test waits for a fix any more (no PENDING, no `Cover::Pending`).

use borsh::BorshDeserialize;
use permutation_chain_svm_tests::cover::{self, Code, Cover};
use permutation_chain_svm_tests::error::ChainError;
use permutation_chain_svm_tests::instruction::ChainInstruction;
use std::collections::BTreeMap;

fn release_check() -> bool {
    std::env::var("RELEASE_CHECK").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// One `#[test]` of `tests/<file>.rs`.
#[derive(Debug)]
struct TestFn {
    /// `file::function`.
    name: String,
    /// The `#[ignore = "…"]` reason (`Some("")` for a bare `#[ignore]`).
    ignored: Option<String>,
    /// From `fn` to the next `#[test]` or the end of the file.
    body: String,
}

/// `src` without `//` and `/* */` comments (string and char literals kept).
fn strip_comments(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '"' {
            out.push(c);
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' {
                    out.push(b[i]);
                    i += 1;
                }
                out.push(b[i]);
                i += 1;
            }
            if i < b.len() {
                out.push('"');
            }
            i += 1;
        } else if c == '\'' && (next == Some('\\') || b.get(i + 2) == Some(&'\'')) {
            // A char literal ('"', '\n'), not a lifetime.
            let end = (i + 1..b.len())
                .find(|j| b[*j] == '\'' && *j > i + 1)
                .unwrap_or(b.len() - 1);
            out.extend(&b[i..=end]);
            i = end + 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Every `#[test]` in `tests/*.rs`.
fn test_index() -> Vec<TestFn> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    let mut out = vec![];
    for path in files {
        let file = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = strip_comments(&std::fs::read_to_string(&path).unwrap());
        // A test starts at a line that is exactly `#[test]`.
        let lines: Vec<&str> = src.lines().collect();
        let starts: Vec<usize> = (0..lines.len())
            .filter(|i| lines[*i].trim() == "#[test]")
            .collect();
        for (k, at) in starts.iter().enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(lines.len());
            let f = (at + 1..end)
                .find(|i| lines[*i].trim_start().starts_with("fn "))
                .unwrap_or_else(|| panic!("{file}: #[test] without fn"));
            let name: String = lines[f].trim_start()[3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let ignored = lines[at + 1..f]
                .iter()
                .map(|l| l.trim())
                .find(|l| l.starts_with("#[ignore"))
                .map(|l| {
                    l.split_once('"')
                        .and_then(|(_, r)| r.rsplit_once('"'))
                        .map(|(reason, _)| reason.to_string())
                        .unwrap_or_default()
                });
            out.push(TestFn {
                name: format!("{file}::{name}"),
                ignored,
                body: lines[f..end].join("\n"),
            });
        }
    }
    out
}

/// One instruction per tag: `[tag] ‖ zeros`, deserialized as a prefix, for
/// `tag = 0..` up to the first tag that is no variant. A variant whose
/// payload does not decode from zeros fails here, rather than hiding
/// itself and every later tag from the covers.
fn samples() -> Vec<ChainInstruction> {
    let mut out = vec![];
    for tag in 0..=255u8 {
        let mut data = vec![0u8; 4097];
        data[0] = tag;
        match ChainInstruction::deserialize(&mut &data[..]) {
            Ok(ix) => {
                assert_eq!(borsh::to_vec(&ix).unwrap()[0], tag);
                out.push(ix);
            }
            Err(e) if e.to_string().starts_with("Unexpected variant tag") => break,
            Err(e) => panic!("tag {tag} is a variant but does not decode from zeros ({e}): give samples() a payload for it"),
        }
    }
    out
}

/// Everything the tables name: (label, covers).
fn all_covers() -> Vec<(String, &'static [Cover])> {
    let mut all: Vec<(String, &'static [Cover])> = samples()
        .iter()
        .enumerate()
        .map(|(tag, ix)| {
            let name = format!("{ix:?}");
            let name = name.split([' ', '{', '(']).next().unwrap().to_string();
            (format!("tag {tag} {name}"), cover::covered_by(ix))
        })
        .collect();
    all.push(("undelegate callback".into(), cover::CALLBACK));
    all.push(("not an instruction".into(), cover::NOT_AN_INSTRUCTION));
    all
}

fn needle(code: &Code) -> String {
    match code {
        Code::Chain(e) => format!("E::{}", e.name()),
        Code::Token => "assert_token_err(".into(),
        Code::Program(n) => (*n).into(),
        Code::Lands(n) => (*n).into(),
    }
}

#[test]
fn every_instruction_is_covered() {
    let index = test_index();
    let by_name: BTreeMap<&str, &TestFn> = index.iter().map(|t| (t.name.as_str(), t)).collect();
    let mut problems = vec![];
    let mut pending = vec![];
    for (label, covers) in all_covers() {
        let (mut lands, mut refused, mut retired) = (false, false, false);
        for cover in covers {
            let (test, codes) = match cover {
                Cover::Pending(wp) => {
                    pending.push(format!("{label}: {wp}"));
                    lands = true;
                    refused = true;
                    continue;
                }
                Cover::Test(test, codes) => (*test, *codes),
            };
            let Some(t) = by_name.get(test) else {
                problems.push(format!("{label}: no test {test}"));
                continue;
            };
            if let Some(why) = &t.ignored {
                problems.push(format!(
                    "{label}: {test} is ignored ({why}); covers must name running tests"
                ));
                continue;
            }
            for code in codes {
                let n = needle(code);
                let found = match code {
                    Code::Lands(_) => lands_in(&t.body, &n),
                    _ => t.body.contains(&n),
                };
                if !found {
                    problems.push(format!(
                        "{label}: {test} does not assert {code:?} (its body needs {n:?}{})",
                        if matches!(code, Code::Lands(_)) {
                            " and .expect( or .unwrap() in the same statement"
                        } else {
                            ""
                        }
                    ));
                }
                match code {
                    Code::Lands(_) => lands = true,
                    Code::Chain(ChainError::Retired) => (retired, refused) = (true, true),
                    _ => refused = true,
                }
            }
        }
        if !refused {
            problems.push(format!("{label}: no negative test"));
        }
        if !lands && !retired && label != "not an instruction" {
            problems.push(format!("{label}: no positive test"));
        }
    }
    for p in &pending {
        println!("PENDING cover: {p}");
    }
    if release_check() {
        assert!(
            pending.is_empty(),
            "RELEASE_CHECK: instructions without tests: {pending:#?}"
        );
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// `body` with the inside of every string literal blanked (same length), so
/// brackets and `;` in messages do not count.
fn mask_strings(body: &str) -> Vec<u8> {
    let mut out = body.as_bytes().to_vec();
    let mut i = 0;
    while i < out.len() {
        if out[i] == b'"' {
            i += 1;
            while i < out.len() && out[i] != b'"' {
                let skip = if out[i] == b'\\' { 2 } else { 1 };
                for k in i..(i + skip).min(out.len()) {
                    out[k] = b' ';
                }
                i += skip;
            }
        }
        i += 1;
    }
    out
}

/// The end of the statement at `from`: the first `;` outside the brackets
/// opened after `from`, or a `}` that closes a block around it.
fn statement_end(b: &[u8], from: usize) -> usize {
    let mut depth = 0i32;
    for (i, c) in b.iter().enumerate().skip(from) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b'}' if depth <= 0 => return i,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth <= 0 => return i,
            _ => {}
        }
    }
    b.len()
}

/// The start of the statement around `at`: after the `;` or `{` that
/// precedes it at its own level, climbing out of brackets (and of a closure
/// or block whose first statement it is).
fn statement_start(b: &[u8], at: usize) -> usize {
    let mut depth = 0i32;
    for i in (0..at).rev() {
        match b[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => depth -= 1,
            b';' if depth <= 0 => return i + 1,
            _ => {}
        }
    }
    0
}

/// Whether the statement at `from` unwraps a result.
fn unwraps(b: &[u8], from: usize) -> bool {
    let stmt = String::from_utf8_lossy(&b[from..statement_end(b, from)]).to_string();
    stmt.contains(".expect(") || stmt.contains(".unwrap()")
}

/// Whether `body` sends `needle`'s instruction and unwraps that send: the
/// statement that uses it ends in `.expect(`/`.unwrap()`, or it is bound
/// (`let x = …needle…`, a value or a closure) and a later statement that
/// uses `x` does.
fn lands_in(body: &str, needle: &str) -> bool {
    let b = mask_strings(body);
    let text = String::from_utf8_lossy(&b).to_string();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    text.match_indices(needle).any(|(at, _)| {
        if unwraps(&b, at) {
            return true;
        }
        let start = statement_start(&b, at);
        let stmt = text[start..at].trim_start();
        let Some(rest) = stmt.strip_prefix("let ") else {
            return false;
        };
        let rest = rest.strip_prefix("mut ").unwrap_or(rest);
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            return false;
        }
        let after = statement_end(&b, at);
        text[after..].match_indices(name.as_str()).any(|(k, _)| {
            let k = after + k;
            let bounded = !word(b[k - 1]) && b.get(k + name.len()).is_none_or(|c| !word(*c));
            bounded && unwraps(&b, k)
        })
    })
}

#[test]
fn lands_needs_the_unwrap_on_its_own_send() {
    let ok = "c.send(vec![s.part_ix(&cp, [0; 4], true)], &[&k])\n    .expect(\"lands; really\");";
    assert!(lands_in(ok, "part_ix("));
    // Bound, then sent and unwrapped: a value or a closure.
    let bound = "let ix = s.part_ix(&cp, vec![1], true);\nc.send(vec![ix], &[&k]).unwrap();";
    assert!(lands_in(bound, "part_ix("));
    let closure = "let go = |c: &mut Chain| {\n    c.send(vec![s.part_ix(&cp, vec![1], true)], &[&k])\n};\nassert_err(go(&mut c), E::X);\ngo(&mut c).expect(\"lands\");";
    assert!(lands_in(closure, "part_ix("));
    // The unwrap is on another statement: not a landing.
    let apart = "let r = c.send(vec![s.part_ix(&cp, vec![1], true)], &[&k]);\nassert!(r.is_err());\nlet x = other().unwrap();";
    assert!(!lands_in(apart, "part_ix("));
    let only_refused = "let go = || c.send(vec![s.part_ix(&cp, vec![1], true)], &[&k]);\nassert_err(go(), E::X);\nlet gone = 1;\nx.unwrap();";
    assert!(!lands_in(only_refused, "part_ix("));
}

#[test]
fn every_error_is_asserted() {
    let covers = all_covers();
    let asserted = |e: ChainError| {
        covers.iter().any(|(_, list)| {
            list.iter().any(|c| matches!(c, Cover::Test(_, codes) if codes.iter().any(|x| matches!(x, Code::Chain(y) if *y == e))))
        })
    };
    let missing: Vec<&str> = ChainError::ALL
        .iter()
        .filter(|e| !asserted(**e) && !cover::EXEMPT.iter().any(|(x, _)| x == *e))
        .map(|e| e.name())
        .collect();
    assert!(
        missing.is_empty(),
        "error codes no test asserts: {missing:?}"
    );
    for (e, why) in cover::EXEMPT {
        println!("exempt: {} ({why})", e.name());
    }
}

#[test]
fn ignores_are_accounted_for() {
    let index = test_index();
    let pending = cover::pending();
    let mut problems = vec![];
    for t in index.iter().filter(|t| t.ignored.is_some()) {
        let why = t.ignored.as_deref().unwrap();
        let p = pending.iter().filter(|(n, _)| *n == t.name).count();
        let o: Vec<_> = cover::OPT_IN.iter().filter(|(n, _)| *n == t.name).collect();
        match (p, o.len()) {
            (1, 0) => {
                let wp = pending.iter().find(|(n, _)| *n == t.name).unwrap().1;
                if !why.starts_with(&format!("until {wp}")) {
                    problems.push(format!(
                        "{}: reason {why:?} must start with \"until {wp}\"",
                        t.name
                    ));
                }
            }
            (0, 1) => {
                if !why.contains(o[0].1) {
                    problems.push(format!("{}: reason {why:?} must name {}", t.name, o[0].1));
                }
            }
            _ => problems.push(format!(
                "{}: ignored ({why:?}) but on {p} PENDING and {} OPT_IN lines",
                t.name,
                o.len()
            )),
        }
    }
    for (name, _) in pending.iter().chain(cover::OPT_IN) {
        if !index.iter().any(|t| t.name == *name && t.ignored.is_some()) {
            problems.push(format!(
                "{name}: on a ledger but not an ignored test (un-ignored tests leave the ledger)"
            ));
        }
    }
    let mut by_wp: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, wp) in &pending {
        by_wp.entry(wp).or_default().push(name);
    }
    for (wp, tests) in &by_wp {
        println!("PENDING {wp}: {}", tests.join(", "));
    }
    assert!(problems.is_empty(), "{problems:#?}");
    if release_check() {
        assert!(
            pending.is_empty(),
            "RELEASE_CHECK: tests still wait for their fix: {by_wp:#?}"
        );
    }
}

/// Every signer check runs with real keypairs: these files never turn
/// signature verification off.
#[test]
fn signer_tests_use_sigverify() {
    for file in ["registration", "settlement", "roster"] {
        let path = format!("{}/tests/{file}.rs", env!("CARGO_MANIFEST_DIR"));
        let src = strip_comments(&std::fs::read_to_string(&path).unwrap());
        assert!(
            !src.contains("new_opts(false)"),
            "{file}.rs turns sigverify off"
        );
    }
}

/// The play server and this suite build the chain crate without the
/// `program` feature, and the always-compiled modules stay outside it: code
/// they call must not live in `processor` (the builds are the real check;
/// this names the rule).
#[test]
fn server_never_enables_program() {
    for (who, manifest) in [
        (
            "permutation-server",
            include_str!("../../../permutation-server/Cargo.toml"),
        ),
        ("svm-tests", include_str!("../Cargo.toml")),
    ] {
        let deps: Vec<&str> = manifest
            .lines()
            .filter(|l| l.trim_start().starts_with("permutation-chain"))
            .collect();
        assert_eq!(
            deps.len(),
            1,
            "{who}: one permutation-chain dependency, no dev-dependency: {deps:?}"
        );
        assert!(
            deps[0].contains("default-features = false"),
            "{who}: {}",
            deps[0]
        );
        assert!(!deps[0].contains("\"program\""), "{who}: {}", deps[0]);
    }
    let lib = include_str!("../../src/lib.rs");
    let lines: Vec<&str> = lib.lines().map(str::trim).collect();
    let gated = |m: &str| {
        let at = lines
            .iter()
            .position(|l| *l == format!("pub mod {m};"))
            .unwrap_or_else(|| panic!("no module {m}"));
        at > 0 && lines[at - 1].contains("feature = \"program\"")
    };
    assert!(gated("processor"), "processor needs the program feature");
    for m in [
        "error",
        "finalize",
        "heap",
        "instruction",
        "payout",
        "seat",
        "state",
        "token",
    ] {
        assert!(!gated(m), "{m} must be always compiled");
    }
    for m in ["randomness", "lifecycle", "rules"] {
        if lines.contains(&format!("pub mod {m};").as_str()) {
            assert!(!gated(m), "{m} must be always compiled");
        }
    }
}

#[test]
fn the_parser_ignores_comments_and_strings() {
    let src =
        "// E::Hidden\nlet a = \"// not a comment\"; /* E::Gone */ let q = '\"'; fn f<'a>() {}\n";
    let s = strip_comments(src);
    assert!(!s.contains("E::Hidden") && !s.contains("E::Gone"));
    assert!(s.contains("\"// not a comment\"") && s.contains("'\"'") && s.contains("fn f<'a>"));
}
