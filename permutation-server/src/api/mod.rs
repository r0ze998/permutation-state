//! JSON API: order DTOs, the per-civ world view, and preview payloads.
//!
//! Names on the wire are the engine's own enum names (`"Granary"`,
//! `"Pikeman"`, `"Astronomy"`), so clients and agents never need a second
//! table of ids. Blocked reasons are `{ "code": "...", ...data }`; clients
//! localise by code.

use permutation_rules::buildings::{Building, BUILDINGS};
use permutation_rules::gov::{MemberId, Role, NOBODY};
use permutation_rules::orders::Side;
use permutation_rules::state::Focus;
use permutation_rules::tech::{Tech, TECHS};
use permutation_rules::units::{UnitType, UNIT_STATS};
use serde_json::{json, Value};

mod blocked;
mod dto;
mod gov;
mod previews;
mod world;

pub use blocked::*;
pub use dto::*;
pub use gov::*;
pub use previews::*;
pub use world::*;

// ------------------------------------------------------------------ names

pub(super) fn name<T: core::fmt::Debug>(v: T) -> String {
    format!("{v:?}")
}

pub fn parse_tech(s: &str) -> Option<Tech> {
    TECHS.iter().map(|t| t.tech).find(|t| name(t) == s)
}
pub fn parse_building(s: &str) -> Option<Building> {
    BUILDINGS.iter().map(|b| b.building).find(|b| name(b) == s)
}
pub fn parse_unit(s: &str) -> Option<UnitType> {
    UNIT_STATS.iter().map(|u| u.unit).find(|u| name(u) == s)
}
pub(super) fn parse_focus(s: &str) -> Option<Focus> {
    [
        Focus::Balanced,
        Focus::Food,
        Focus::Production,
        Focus::Gold,
        Focus::Science,
    ]
    .into_iter()
    .find(|f| name(f) == s)
}
pub(super) fn parse_side(s: &str) -> Option<Side> {
    match s {
        "Buy" => Some(Side::Buy),
        "Sell" => Some(Side::Sell),
        _ => None,
    }
}

// ------------------------------------------------------------------ V5: nations and governance

/// A member id on the wire: `null` for nobody (the acting official).
pub fn member_ref(m: MemberId) -> Value {
    if m == NOBODY {
        Value::Null
    } else {
        json!(m)
    }
}

pub fn parse_role(s: &str) -> Option<Role> {
    Role::ALL.into_iter().find(|r| name(r) == s)
}
