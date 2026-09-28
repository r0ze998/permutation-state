//! Clash reports (contract §8.4 `/h/clash/{P},{Q}/{bell}`): the inputs, the
//! seed and its anchor, the digests the program logged, the decoded
//! fighters and fates, and `heraldCheck` — the clash **recomputed natively
//! with `clash::resolve_clash`** and compared with the outcome digest the
//! CLASH record carries.
//!
//! Recomputing needs the `ClashInput` that ResolveFromInputs builds from the
//! Province (before the resolve) and the ClashInputs account. That builder is
//! the program's (W4-A, `permutation-frontier/src/proc/clash.rs`, a stub in
//! wave 3), so the herald takes it through the [`ClashBuilder`] trait:
//! [`Provisional`] follows the §5.11 ResolveFromInputs text over the
//! `frontier-abi` entry codec until W4-A exports the program's builder, and
//! the herald then plugs that in (handover note in `W3-D-NOTES.md`). The
//! report names the builder it used, and a builder that cannot rebuild the
//! input gives `heraldCheck: "unchecked"` with the reason instead of a
//! false `MISMATCH` alarm.

use permutation_rules::frontier::clash::{self, ClashOutcome, Fate};

/// Builds the kernel input of one province-bell and resolves it.
pub trait ClashBuilder: Send + Sync {
    /// Named in every report (`builder`).
    fn name(&self) -> &'static str;
    /// The outcome of the clash of `bell` from the Province bytes before
    /// ResolveFromInputs, the ClashInputs bytes after it and the seed.
    fn recompute(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<ClashOutcome, String>;
}

/// The program's ClashInput builder (`fclient::clash_model`, one
/// transcription of W4-A's `proc::clash::model::build` shared with the
/// verifier; integ-W4 `3acc6b1` aligned the herald's own copy first, the
/// wave-4 review moved it to fclient). Moving the program's model into
/// `frontier-abi` stays W4-A's request D8 for W5.
pub struct Provisional;

pub use fclient::clash_model::{terrain_of, BuiltInput, CAMP_DOMAIN, TERRAINS};

impl Provisional {
    /// The input the builder hands to the kernel.
    pub fn build(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<BuiltInput, String> {
        if inputs.is_empty() {
            return Err("inputs: empty".into());
        }
        fclient::clash_model::build(province_before, inputs, bell, seed)
    }
}

impl ClashBuilder for Provisional {
    fn name(&self) -> &'static str {
        "provisional-w3d"
    }
    fn recompute(
        &self,
        province_before: &[u8],
        inputs: &[u8],
        bell: u32,
        seed: &[u8; 32],
    ) -> Result<ClashOutcome, String> {
        let b = self.build(province_before, inputs, bell, seed)?;
        clash::resolve_clash(&clash::frontier_ruleset(), &b.input()).map_err(|e| format!("{e:?}"))
    }
}

/// A fate's name in reports.
pub fn fate_name(f: &Fate) -> &'static str {
    match f {
        Fate::Stays { .. } => "Stays",
        Fate::Withdrew { .. } => "Withdrew",
        Fate::Bounced => "Bounced",
        Fate::Retreated => "Retreated",
        Fate::Destroyed => "Destroyed",
    }
}

/// The fighters of an outcome as report JSON.
pub fn fighters_json(o: &ClashOutcome) -> serde_json::Value {
    serde_json::Value::Array(
        o.fighters
            .iter()
            .map(|f| {
                let tile = match f.fate {
                    Fate::Stays { tile } | Fate::Withdrew { tile } => Some(tile),
                    _ => None,
                };
                serde_json::json!({
                    "id": f.id.to_string(), "arrival": f.arrival, "troops": f.troops,
                    "stamina": f.stamina, "fate": fate_name(&f.fate), "tile": tile, "engaged": f.engaged,
                })
            })
            .collect(),
    )
}
