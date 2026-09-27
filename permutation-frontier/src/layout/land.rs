//! Land: the Province (4,096, §5.3) and its sub-records: site mirror, entry,
//! resolve summary, camp, ticket cohort. Offsets:
//! `frontier_abi::layout::province`; the entry ↔ kernel `Host` codec is
//! `frontier_abi::entry`.

pub use frontier_abi::entry::{find_entry, read_entry, write_entry, Entry, EntryOp};
pub use frontier_abi::layout::province::{camp, cohort, entry, province, site, summary};
