//! The program's `sol_log_data` records (`Program data:` lines): PS_INPUT,
//! PS_TICK, PS_SALTS, PS_COMMITS, PS_SEAT, PS_OPEN, PS_GENESIS, PS_HISTORY,
//! PS_TALK.

use base64::Engine;

/// Every record in `logs` whose first field is `tag`, as its fields.
pub fn records(logs: &[String], tag: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let b64 = base64::engine::general_purpose::STANDARD;
    logs.iter()
        .filter_map(|l| l.strip_prefix("Program data: "))
        .map(|l| {
            l.split(' ')
                .map(|f| b64.decode(f).unwrap())
                .collect::<Vec<_>>()
        })
        .filter(|f| f.first().is_some_and(|t| t.as_slice() == tag))
        .collect()
}

/// The one record `tag` in `logs` (panics if there is not exactly one).
pub fn record(logs: &[String], tag: &[u8]) -> Vec<Vec<u8>> {
    let mut all = records(logs, tag);
    assert_eq!(
        all.len(),
        1,
        "{} records of {}",
        all.len(),
        String::from_utf8_lossy(tag)
    );
    all.pop().unwrap()
}

/// The log bytes a transaction used, as the runtime's log collector counts
/// them (its cap is 10 000 bytes; a truncated log ends in "Log truncated").
pub fn log_bytes(logs: &[String]) -> usize {
    logs.iter().map(|l| l.len()).sum()
}

/// Whether the runtime cut the logs at its 10 000-byte cap.
pub fn log_truncated(logs: &[String]) -> bool {
    logs.iter().any(|l| l == "Log truncated")
}
