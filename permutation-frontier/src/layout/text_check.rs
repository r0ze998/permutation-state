//! Host test: every offset the contract text states in §5.3 starts a field
//! of the `frontier-abi` layout it describes, and every stated size is the
//! layout's (the text-driven check integ-W1 left to W2-A; W1-E's
//! `offsets_match_the_contract_text` transcribes 63 offsets by hand).
//!
//! The parser reads `docs/frontier/m1/M1-CONTRACT.md` between "### 5.3" and
//! "### 5.4": `**Name (size…)**` paragraphs (inline `· off field` lists) and
//! their `| Off | … |` tables, plus the sub-record paragraphs `Transit
//! record (96):`, `Site mirror (64):`, `Entry (48):` and `Arrival record
//! (40):`. From a table row it takes the first cell (and, in the Season's
//! four-column table, the third); from inline text every fragment between
//! ` · ` that starts with a number; `a..b` ranges give `a`.

use alloc::string::String;
use alloc::vec::Vec;

use frontier_abi::layout::{self as L, Field};

const CONTRACT: &str = include_str!("../../../docs/frontier/m1/M1-CONTRACT.md");

fn num(s: &str) -> Option<usize> {
    let t = s.trim().trim_start_matches('|').trim();
    let first = t.split(char::is_whitespace).next()?;
    let first = first.split("..").next()?;
    let digits: String = first.chars().filter(|c| *c != ',').collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Offsets stated in one inline list (`· 64 name … · 66 …`).
fn inline(text: &str, out: &mut Vec<usize>) {
    for frag in text.split(" · ") {
        if let Some(n) = num(frag) {
            out.push(n);
        }
    }
}

struct Layout {
    name: &'static str,
    size: usize,
    fields: &'static [Field],
}

fn layouts() -> Vec<Layout> {
    let mut v: Vec<Layout> = L::AccountKind::ALL
        .iter()
        .map(|k| Layout {
            name: k.name(),
            size: k.size(),
            fields: k.fields(),
        })
        .collect();
    v.push(Layout {
        name: "Transit record",
        size: L::player::transit::SIZE,
        fields: L::player::transit::FIELDS,
    });
    v.push(Layout {
        name: "Site mirror",
        size: L::province::site::SIZE,
        fields: L::province::site::FIELDS,
    });
    v.push(Layout {
        name: "Entry",
        size: L::province::entry::SIZE,
        fields: L::province::entry::FIELDS,
    });
    v.push(Layout {
        name: "Arrival record",
        size: L::clash::arrival::SIZE,
        fields: L::clash::arrival::FIELDS,
    });
    v
}

/// `(layout name, stated size, stated offsets)` for every layout in §5.3.
fn stated(contract: &str) -> Vec<(String, usize, Vec<usize>)> {
    let start = contract.find("### 5.3").expect("§5.3");
    let end = contract[start..].find("### 5.4").expect("§5.4") + start;
    let sec = &contract[start..end];
    let mut out: Vec<(String, usize, Vec<usize>)> = Vec::new();
    let subs = [
        "Transit record (",
        "Site mirror (",
        "Entry (",
        "Arrival record (",
    ];
    for line in sec.lines() {
        // A paragraph that opens a layout: `**Name (size…)**` or a sub-record.
        let mut rest = line;
        loop {
            let open = if let Some(p) = rest.strip_prefix("**") {
                p.find(" (").map(|i| (p[..i].to_string(), &p[i + 2..]))
            } else {
                subs.iter().find_map(|s| {
                    rest.find(s)
                        .filter(|&at| at == 0 || rest[..at].ends_with(". "))
                        .map(|at| {
                            let name = s.trim_end_matches(" (").to_string();
                            (name, &rest[at + s.len()..])
                        })
                })
            };
            let Some((name, after)) = open else { break };
            let size_txt: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .filter(|c| *c != ',')
                .collect();
            let Ok(size) = size_txt.parse::<usize>() else {
                break;
            };
            // body: after the closing `)**` / `):`, up to the next sub-record
            let body_at = after.find(')').map(|i| i + 1).unwrap_or(0);
            let mut body = &after[body_at..];
            let mut next = None;
            for s in subs {
                if let Some(i) = body.find(&format!(". {s}")) {
                    next = Some(next.map_or(i, |n: usize| n.min(i)));
                }
            }
            let tail = next.map(|i| &body[i + 2..]);
            if let Some(i) = next {
                body = &body[..i];
            }
            let mut offs = Vec::new();
            inline(body, &mut offs);
            out.push((name, size, offs));
            match tail {
                Some(t) => rest = t,
                None => break,
            }
        }
        // Table rows belong to the last opened layout.
        if line.starts_with("| ") && !line.starts_with("| Off") && !line.starts_with("|---") {
            if let Some(last) = out.last_mut() {
                let cells: Vec<&str> = line.split('|').map(str::trim).collect();
                // cells[0] is empty (leading '|')
                if let Some(n) = cells.get(1).and_then(|c| num(c)) {
                    last.2.push(n);
                }
                if last.0 == "Season" {
                    if let Some(n) = cells.get(3).and_then(|c| num(c)) {
                        last.2.push(n);
                    }
                } else {
                    // mixed inline cells (Citizen: `faction u8 · 137 flags u8 · …`)
                    for c in cells.iter().skip(2) {
                        let mut v = Vec::new();
                        inline(c, &mut v);
                        // a cell with ` · ` lists more fields after its first
                        if c.contains(" · ") {
                            last.2.extend(v);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Checks the text; `Err` names the first mismatch. Returns the number of
/// offsets checked.
fn check(contract: &str) -> Result<usize, String> {
    let layouts = layouts();
    let stated = stated(contract);
    let mut checked = 0;
    let mut seen = Vec::new();
    for (name, size, offs) in &stated {
        let Some(l) = layouts.iter().find(|l| l.name == name) else {
            // `SealVerdict` (removed) has no layout.
            if name != "SealVerdict" {
                return Err(format!("unknown layout {name}"));
            }
            continue;
        };
        if *size != l.size {
            return Err(format!("{name}: stated size {size}, abi {}", l.size));
        }
        let hdr = if l.size >= 64
            && L::AccountKind::ALL
                .iter()
                .any(|k| k.name() == name && k.chained())
        {
            64
        } else if L::AccountKind::ALL.iter().any(|k| k.name() == name) {
            16
        } else {
            0
        };
        for &o in offs {
            // Offsets inside the header are stated as `H`/`SH`, never as numbers
            // (Season's `H(0..64)` gives 0).
            if o < hdr && o != 0 {
                continue;
            }
            if !(o == 0 && hdr > 0 || l.fields.iter().any(|f| f.off == o)) {
                return Err(format!(
                    "{name}: the contract states a field at {o}; frontier-abi has none starting there"
                ));
            }
            checked += 1;
        }
        seen.push(name.clone());
    }
    for l in &layouts {
        if !seen.iter().any(|s| s == l.name) {
            return Err(format!("{} not found in §5.3", l.name));
        }
    }
    for (name, _, offs) in &stated {
        if name != "SealVerdict" && offs.len() < 3 {
            return Err(format!("{name}: only {} offsets parsed", offs.len()));
        }
    }
    Ok(checked)
}

#[test]
fn every_offset_in_the_contract_text_starts_an_abi_field() {
    let n = check(CONTRACT).unwrap();
    std::eprintln!("contract §5.3: {n} offsets checked");
    // Well above W1-E's 63 hand-transcribed offsets.
    assert!(n >= 250, "{n} offsets checked");
}

/// Controls: a shifted offset, a wrong size and a missing layout in the
/// text each fail the check.
#[test]
fn the_check_catches_a_shifted_offset_or_size() {
    let shifted = CONTRACT.replacen(
        "· 66 rsv u16 · 68 last_ring_open_bell",
        "· 67 rsv u16 · 68 last_ring_open_bell",
        1,
    );
    assert_ne!(shifted, CONTRACT);
    assert!(check(&shifted).unwrap_err().contains("Frontier"));
    let resized = CONTRACT.replacen("**BeaconLog (128)**", "**BeaconLog (136)**", 1);
    assert!(check(&resized).unwrap_err().contains("BeaconLog"));
    let entry = CONTRACT.replacen("· 36 pend_op u8", "· 35 pend_op u8", 1);
    assert!(check(&entry).unwrap_err().contains("Entry"));
    let season = CONTRACT.replacen("| 300 | reveal_cu_limit", "| 302 | reveal_cu_limit", 1);
    assert!(check(&season).unwrap_err().contains("Season"));
}
