//! The program under test: the `test-beacon` build of `permutation-frontier`
//! (I-53: it pins the test key, so day-scale gates do not wait for drand
//! archives).
//!
//! `PSF_FRONTIER_SO=<path>` names a prebuilt `.so` (the integrator's gate
//! may pass the one it built). Otherwise the `.so` is built once per test
//! process with `scripts/build-frontier.sh --features test-beacon` (an
//! incremental `cargo-build-sbf`: ≈ 18 s cold, a few seconds warm), so a
//! gate never runs a stale program after a merge. `ITEST_NO_BUILD=1` skips
//! the build and uses the file as it is.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// The repository root (`frontier-node/crates/itest/../../..`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

/// Where `build-frontier.sh --features test-beacon` writes the `.so`.
pub fn default_so() -> PathBuf {
    repo_root().join("permutation-frontier/target/deploy-test-beacon/permutation_frontier.so")
}

/// The test-beacon program: path, bytes' sha256 and how it was obtained.
#[derive(Clone, Debug)]
pub struct So {
    pub path: PathBuf,
    pub sha256: String,
    pub origin: String,
}

fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

fn build() -> Result<String, String> {
    let root = repo_root();
    let script = root.join("scripts/build-frontier.sh");
    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!(
        "{home}/.local/share/solana/install/active_release/bin:{}",
        std::env::var("PATH").unwrap_or_default()
    );
    let out = std::process::Command::new("bash")
        .arg(&script)
        .args(["--features", "test-beacon"])
        .current_dir(&root)
        .env("PATH", path)
        .output()
        .map_err(|e| format!("{}: {e}", script.display()))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        return Err(format!(
            "build-frontier.sh --features test-beacon failed ({}):\n{text}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(text)
}

fn locate() -> Result<So, String> {
    let (path, origin) = match std::env::var("PSF_FRONTIER_SO") {
        Ok(p) if !p.is_empty() => (PathBuf::from(p), "PSF_FRONTIER_SO".to_string()),
        _ if std::env::var("ITEST_NO_BUILD").is_ok_and(|v| v == "1") => {
            (default_so(), "prebuilt (ITEST_NO_BUILD=1)".to_string())
        }
        _ => {
            let log = build()?;
            let features = log
                .lines()
                .find(|l| l.starts_with("features"))
                .unwrap_or("")
                .to_string();
            if !features.contains("test-beacon") {
                return Err(format!(
                    "the build did not report the test-beacon feature:\n{log}"
                ));
            }
            (
                default_so(),
                "scripts/build-frontier.sh --features test-beacon".to_string(),
            )
        }
    };
    let b = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Only a test-beacon build accepts the test key (I-53).
    if !b
        .windows(b"PSF_TEST_BEACON_BUILD".len())
        .any(|w| w == b"PSF_TEST_BEACON_BUILD")
    {
        return Err(format!(
            "{} is not a test-beacon build (marker PSF_TEST_BEACON_BUILD absent)",
            path.display()
        ));
    }
    Ok(So {
        path,
        sha256: sha(&b),
        origin,
    })
}

/// The test-beacon `.so`, located or built once per process.
pub fn test_beacon_so() -> Result<So, String> {
    static SO: OnceLock<Result<So, String>> = OnceLock::new();
    SO.get_or_init(locate).clone()
}
