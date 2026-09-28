//! The recorded herald fixtures (`fixtures/herald-recorded`): loading a
//! recording and the marches its wallets plan on it (W6-C).
//!
//! One rule for two readers: `tests/herald_fixtures.rs` checks the plans
//! against the kernel, and `itest::day`'s recorder keeps a recording only
//! when it **drives an unrested march** (a wallet plans a march with its
//! host's recorded stamina, not only after resting the roster: DECISIONS
//! O11, the integ-W5 review found 0 at bells 60, 120 and 140).
//!
//! The decision is the policy's with `Memory::default()`, a session, the
//! direct route and the province caught up to the recording's bell (an idle
//! province may sit a keeper skip batch behind; the bot would nudge first).

use std::collections::BTreeMap;
use std::path::Path;

use fclient::abi::layout::entry as le;
use fclient::{Address, Signer};
use serde_json::Value;

use crate::keys;
use crate::obs::{BellView, MeView, Observation, Overview, ProvinceView, SeasonView};
use crate::policy::{decide, Ctx, DepartPlan, Intent, Memory};
use crate::profile::AgentSpec;
use crate::Persona;

/// One recorded wallet.
#[derive(Clone, Debug)]
pub struct Wallet {
    pub spec: AgentSpec,
    pub wallet: Address,
    pub me: MeView,
}

/// A loaded recording.
#[derive(Clone, Debug)]
pub struct Recording {
    pub seed: u64,
    pub recorded_bell: u32,
    pub season: SeasonView,
    pub provinces: BTreeMap<(i16, i16), ProvinceView>,
    pub bells: BTreeMap<(u32, u8), BellView>,
    pub overviews: Vec<Overview>,
    pub wallets: Vec<Wallet>,
}

/// A march a recorded wallet plans.
#[derive(Clone, Debug)]
pub struct Planned {
    pub index: u32,
    /// The observation the plan was decided on.
    pub obs: Observation,
    pub plan: DepartPlan,
    /// Planned only once the wallet's roster was rested (stamina at the cap).
    pub rested: bool,
}

/// What the recording's wallets plan.
#[derive(Clone, Debug, Default)]
pub struct Plans {
    /// Joined wallets decided.
    pub decided: usize,
    pub marches: Vec<Planned>,
}

impl Plans {
    /// Marches planned with the hosts' recorded stamina.
    pub fn unrested(&self) -> usize {
        self.marches.iter().filter(|m| !m.rested).count()
    }
    pub fn rested(&self) -> usize {
        self.marches.iter().filter(|m| m.rested).count()
    }
}

fn read_json(p: &Path) -> Result<Value, String> {
    let b = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display()))
}

fn files(d: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for e in std::fs::read_dir(d).map_err(|e| format!("{}: {e}", d.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            files(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// Loads a recording (`index.json` and the `h/` tree).
pub fn load(dir: &Path) -> Result<Recording, String> {
    let idx = read_json(&dir.join("index.json"))?;
    let seed = idx["seed"].as_u64().ok_or("index.json: seed")?;
    let recorded_bell = idx["recordedBell"]
        .as_u64()
        .ok_or("index.json: recordedBell")? as u32;
    let season = SeasonView::from_json(&read_json(&dir.join("h/season.json"))?)
        .map_err(|e| format!("season: {e:?}"))?;
    let mut provinces = BTreeMap::new();
    let mut bells = BTreeMap::new();
    let mut overviews = vec![];
    let mut mes: BTreeMap<String, MeView> = BTreeMap::new();
    let mut all = vec![];
    files(&dir.join("h"), &mut all)?;
    for p in &all {
        let rel = p
            .strip_prefix(dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("h/overview/") {
            let b = std::fs::read(p).map_err(|e| format!("{rel}: {e}"))?;
            overviews.push(Overview::decode(&b).map_err(|e| format!("{rel}: {e:?}"))?);
        } else if rel.starts_with("h/province/") {
            let v = ProvinceView::from_json(&read_json(p)?).map_err(|e| format!("{rel}: {e:?}"))?;
            provinces.insert(v.coord(), v);
        } else if rel.starts_with("h/bell/") {
            let v = BellView::from_json(&read_json(p)?).map_err(|e| format!("{rel}: {e:?}"))?;
            bells.insert((v.bell, v.region), v);
        } else if let Some(w) = rel.strip_prefix("h/me/") {
            let v = MeView::from_json(&read_json(p)?).map_err(|e| format!("{rel}: {e:?}"))?;
            mes.insert(w.trim_end_matches(".json").to_string(), v);
        }
    }
    let mut wallets = vec![];
    for w in idx["wallets"].as_array().ok_or("index.json: wallets")? {
        let index = w["index"].as_u64().ok_or("wallet index")? as u32;
        let wallet = keys::wallet(seed, index).pubkey();
        let arch = crate::ARCHS
            .iter()
            .copied()
            .find(|a| Some(a.key()) == w["arch"].as_str())
            .ok_or("wallet arch")?;
        let spec = AgentSpec {
            index,
            arch,
            faction: w["faction"].as_u64().ok_or("wallet faction")? as u8,
            join_day: 0,
            join_bell: w["joinBell"].as_u64().ok_or("wallet joinBell")? as u32,
            persona: w["persona"].as_str().and_then(Persona::parse),
        };
        let me = mes
            .remove(&wallet.to_string())
            .ok_or_else(|| format!("no me file for {wallet}"))?;
        wallets.push(Wallet { spec, wallet, me });
    }
    Ok(Recording {
        seed,
        recorded_bell,
        season,
        provinces,
        bells,
        overviews,
        wallets,
    })
}

fn departs(v: &[Intent]) -> Vec<DepartPlan> {
    v.iter()
        .filter_map(|i| match i {
            Intent::Depart(d) => Some(d.as_ref().clone()),
            _ => None,
        })
        .collect()
}

/// The marches the recording's joined wallets plan: with the recorded
/// stamina, and (for a wallet that plans none) with its roster rested.
pub fn plans(r: &Recording, reveal_loaded_limit: u32) -> Plans {
    let now_bell = r.season.bell_at(r.season.latest_unix);
    let caught_up: BTreeMap<_, _> = r
        .provinces
        .iter()
        .map(|(k, v)| {
            let mut v = v.clone();
            v.province.resolved_next = v.province.resolved_next.max(now_bell.saturating_sub(1));
            (*k, v)
        })
        .collect();
    let rested: BTreeMap<_, _> = caught_up
        .iter()
        .map(|(k, v)| {
            let mut v = v.clone();
            for e in v.province.entries.iter_mut() {
                if e.state == le::STATE_ROSTER {
                    e.stamina_value = permutation_rules::frontier::host::STAMINA_CAP;
                    e.stamina_bell = now_bell;
                }
            }
            (*k, v)
        })
        .collect();
    let mut out = Plans::default();
    for w in &r.wallets {
        if w.me.citizen.is_none() {
            continue;
        }
        let mem = Memory::default();
        let c = Ctx {
            spec: &w.spec,
            seed: r.seed,
            wallet: w.wallet,
            mem: &mem,
            reveal_loaded_limit,
            direct: true,
            session: true,
        };
        let o = Observation {
            now: r.season.latest_unix,
            season: r.season.clone(),
            me: w.me.clone(),
            provinces: caught_up.clone(),
            province_bells: BTreeMap::new(),
            overviews: r.overviews.clone(),
            bells: r.bells.clone(),
        };
        let planned = departs(&decide(&o, &c));
        if planned.is_empty() {
            let o = Observation {
                provinces: rested.clone(),
                ..o
            };
            for plan in departs(&decide(&o, &c)) {
                out.marches.push(Planned {
                    index: w.spec.index,
                    obs: o.clone(),
                    plan,
                    rested: true,
                });
            }
        } else {
            for plan in planned {
                out.marches.push(Planned {
                    index: w.spec.index,
                    obs: o.clone(),
                    plan,
                    rested: false,
                });
            }
        }
        out.decided += 1;
    }
    out
}
