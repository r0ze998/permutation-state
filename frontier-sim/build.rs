//! Detects the rules-side `frontier::catalog` module (W1-C, I-56).
//!
//! `tests/catalog_equality.rs` compares that module field by field with
//! `src/model.rs`, which is normative for it. The module lands in the same
//! wave as this test (W1-C merges before W1-D), so the comparison is
//! compiled only once `permutation-rules` declares `pub mod catalog`;
//! before that the test reports that it is waiting, and fails if
//! `FRONTIER_REQUIRE_CATALOG` is set (the integrator's Gate W1 sets it).

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rules_catalog)");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let module = dir.join("../permutation-rules/src/frontier/mod.rs");
    println!("cargo::rerun-if-changed=../permutation-rules/src/frontier/mod.rs");
    println!("cargo::rerun-if-changed=build.rs");
    let declared = std::fs::read_to_string(&module)
        .map(|s| {
            s.lines()
                .any(|l| l.trim_start().starts_with("pub mod catalog"))
        })
        .unwrap_or(false);
    if declared {
        println!("cargo::rustc-cfg=rules_catalog");
    }
}
