//! `frontier-stack` (M1 contract §8.7, §10.3, §12 Gate W5, §13.4 E5): the
//! local stack orchestrator for Mode A runs.
//!
//! | module | what |
//! |---|---|
//! | [`config`] | the stack TOML (`frontier-node/configs/*.toml`) and the `up` flags |
//! | [`ports`] | the §12 port rule over every port a config names |
//! | [`run`] | the run directory, `state.json`, `events.jsonl`, secrets |
//! | [`procs`] | component processes (own process group, logs, signals) |
//! | [`chain`] | `frontier-localnet` over JSON-RPC (status, scale, pause, holds, operator txs) |
//! | [`setup`] | AnnounceSeason → the pre-season at scale 2,000 → CreateSeason, logs, shards |
//! | [`up`] | the start order and the supervisor; `down` |
//! | [`chaos`] | the kill -9 schedule |
//! | [`adversary`] | the `frontier_hold` schedule |
//! | [`verifyrun`] | `verify` and `tamper` on a run |
//! | [`load`] | the herald viewer load and its targets |
//! | [`report`] | the run report |

pub mod adversary;
pub mod chain;
pub mod chaos;
pub mod config;
pub mod load;
pub mod ports;
pub mod procs;
pub mod report;
pub mod run;
pub mod setup;
pub mod toml;
pub mod up;
pub mod verifyrun;
