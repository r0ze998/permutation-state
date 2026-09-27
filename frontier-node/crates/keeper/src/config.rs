//! `keeper.toml` (M1 contract §8.2).
//!
//! ```toml
//! program = "<base58>"
//! season = 1
//! rpc = ["http://127.0.0.1:41010"]
//! drand = ["http://127.0.0.1:41020"]
//! roles = ["beacon", "archive", "rings"]
//! regions = "0-15"
//! reveal_pool = 150
//! delay_pool = 32
//! funders = 4
//! r99_reveals = 4000
//! p_def_milli = 2000
//! p_delay_milli = 500
//! beneficiary = "<base58>"
//! api = "127.0.0.1:41050"
//! token_file = "keeper.token"
//! master_seed_file = "keeper.seed"
//! journal = "keeper.journal.sqlite"
//! ```
//!
//! The parser reads the flat subset of TOML the file uses (`key = value`
//! with strings, integers, floats, booleans and one-line arrays; `#`
//! comments); no dependency is added for it. Unknown keys are refused, so
//! a typo never silently falls back to a default.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use solana_address::Address;

/// Duties (§8.2 `roles`). W2-F runs `beacon` (genesis seed, anchors, seed
/// caches, beacon logs), `rings` (the genesis rings and provinces) and
/// `archive`; the others are accepted and wait for their waves.
pub const ROLES: [&str; 16] = [
    "beacon",
    "reveal",
    "settle-departure",
    "gather",
    "resolve",
    "skip",
    "settle",
    "tickets",
    "explore",
    "archive",
    "close",
    "fold",
    "rings",
    "dormancy",
    "claims",
    "sweep",
];

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Arr(Vec<Value>),
}

/// Parses the flat TOML subset.
pub fn parse_toml(text: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .ok_or(format!("line {}: expected `key = value`", n + 1))?;
        let k = k.trim();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("line {}: bad key `{k}`", n + 1));
        }
        let v = parse_value(v.trim()).map_err(|e| format!("line {}: {e}", n + 1))?;
        if out.insert(k.to_string(), v).is_some() {
            return Err(format!("line {}: `{k}` given twice", n + 1));
        }
    }
    Ok(out)
}

fn strip_comment(s: &str) -> &str {
    let mut in_str = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return &s[..i],
            _ => {}
        }
    }
    s
}

fn parse_value(v: &str) -> Result<Value, String> {
    if let Some(inner) = v.strip_prefix('[') {
        let inner = inner.strip_suffix(']').ok_or("unclosed array")?;
        let mut items = vec![];
        let mut cur = String::new();
        let mut in_str = false;
        for c in inner.chars() {
            match c {
                '"' => {
                    in_str = !in_str;
                    cur.push(c);
                }
                ',' if !in_str => {
                    if !cur.trim().is_empty() {
                        items.push(parse_value(cur.trim())?);
                    }
                    cur.clear();
                }
                _ => cur.push(c),
            }
        }
        if !cur.trim().is_empty() {
            items.push(parse_value(cur.trim())?);
        }
        return Ok(Value::Arr(items));
    }
    if let Some(s) = v.strip_prefix('"') {
        let s = s.strip_suffix('"').ok_or("unclosed string")?;
        if s.contains('"') || s.contains('\\') {
            return Err("escapes are not supported".into());
        }
        return Ok(Value::Str(s.into()));
    }
    match v {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let clean = v.replace('_', "");
    if let Ok(i) = clean.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = clean.parse::<f64>() {
        return Ok(Value::Float(f));
    }
    Err(format!("cannot parse `{v}`"))
}

/// Parses `"0-15"`, `"0,3,7-9"`.
pub fn parse_regions(s: &str) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: u8 = a.parse().map_err(|_| format!("region `{a}`"))?;
        let b: u8 = b.parse().map_err(|_| format!("region `{b}`"))?;
        if a > b || b >= fclient::abi::REGIONS {
            return Err(format!("regions `{part}` outside 0-15"));
        }
        out.extend(a..=b);
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err("no region".into());
    }
    Ok(out)
}

/// The keeper's configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct KeeperConfig {
    pub program: Address,
    pub season_id: u64,
    pub rpc: Vec<String>,
    pub drand: Vec<String>,
    pub roles: Vec<String>,
    pub regions: Vec<u8>,
    pub reveal_pool: usize,
    pub delay_pool: usize,
    pub funders: usize,
    pub r99_reveals: u64,
    pub p_def_milli: u64,
    pub p_delay_milli: u64,
    /// N-class bid and the start of D-class escalation (0.1).
    pub p_low_milli: u64,
    /// `--peace-start <fraction>`: W writes start at `fraction × p_tip`
    /// (O-M1-09 / I-23: off by default, p_start = p_tip).
    pub peace_start: Option<f64>,
    /// Reveal-pool floor override (lamports); default `F_r` from R99 (§8.2).
    pub reveal_floor: Option<u64>,
    /// Delay-pool floor (lamports); default 0.5 SOL (§8.2).
    pub delay_floor: u64,
    pub daily_budget_lamports: u64,
    /// Most lamports the versions of one write may cost (§8.2 duplicates bound).
    pub per_write_cap_lamports: u64,
    pub beneficiary: Address,
    pub api: Option<SocketAddr>,
    pub token_file: Option<PathBuf>,
    pub master_seed_file: Option<PathBuf>,
    pub journal: Option<PathBuf>,
    pub budgets_file: Option<PathBuf>,
    /// Peacetime lag of per-region anchor fallbacks behind the combined
    /// anchor (0 = always together; contested regions use 0).
    pub fallback_lag_slots: u64,
    /// Not landed after this many slots at a bid ≥ p_tip → contested.
    pub contested_slots: u64,
    /// A seed cache not landed after this many slots switches nonce.
    pub nonce_switch_slots: u64,
    /// Bells scanned back for missing anchors at start.
    pub rescan_bells: u32,
    /// Payer care every this many slots (at rest).
    pub care_every_slots: u64,
    /// Pools below their contract minimum are allowed (tests, local drills).
    pub dev: bool,
}

impl KeeperConfig {
    /// Defaults for everything but the program, season and beneficiary.
    pub fn new(program: Address, season_id: u64, beneficiary: Address) -> KeeperConfig {
        KeeperConfig {
            program,
            season_id,
            rpc: vec![],
            drand: vec![],
            roles: vec!["beacon".into(), "rings".into(), "archive".into()],
            regions: (0..fclient::abi::REGIONS).collect(),
            reveal_pool: fclient::payers::MIN_REVEAL,
            delay_pool: fclient::payers::MIN_DELAY,
            funders: fclient::payers::MIN_FUNDERS,
            r99_reveals: fclient::payers::R99_DEFAULT,
            p_def_milli: crate::P_DEF_MILLI,
            p_delay_milli: crate::P_DELAY_MILLI,
            p_low_milli: crate::P_LOW_MILLI,
            peace_start: None,
            reveal_floor: None,
            delay_floor: fclient::payers::DELAY_FLOOR_DEFAULT,
            daily_budget_lamports: 50_000_000_000,
            per_write_cap_lamports: 20_000_000,
            beneficiary,
            api: None,
            token_file: None,
            master_seed_file: None,
            journal: None,
            budgets_file: None,
            fallback_lag_slots: 1,
            contested_slots: 2,
            nonce_switch_slots: 2,
            rescan_bells: 288,
            care_every_slots: 150,
            dev: false,
        }
    }

    pub fn has_role(&self, r: &str) -> bool {
        self.roles.iter().any(|x| x == r)
    }

    /// Reads `keeper.toml` text.
    pub fn from_toml(text: &str) -> Result<KeeperConfig, String> {
        let mut m = parse_toml(text)?;
        let mut take = |k: &str| m.remove(k);
        let s = |v: Value, k: &str| match v {
            Value::Str(s) => Ok(s),
            _ => Err(format!("`{k}` must be a string")),
        };
        let int = |v: Value, k: &str| match v {
            Value::Int(i) if i >= 0 => Ok(i as u64),
            _ => Err(format!("`{k}` must be a non-negative integer")),
        };
        let strs = |v: Value, k: &str| match v {
            Value::Arr(a) => a
                .into_iter()
                .map(|x| match x {
                    Value::Str(s) => Ok(s),
                    _ => Err(format!("`{k}` must be an array of strings")),
                })
                .collect::<Result<Vec<_>, _>>(),
            _ => Err(format!("`{k}` must be an array")),
        };
        let addr = |x: String, k: &str| {
            x.parse::<Address>()
                .map_err(|_| format!("`{k}` is not a base58 address"))
        };
        let program = addr(
            s(take("program").ok_or("`program` is required")?, "program")?,
            "program",
        )?;
        let season = int(take("season").ok_or("`season` is required")?, "season")?;
        let beneficiary = addr(
            s(
                take("beneficiary").ok_or("`beneficiary` is required")?,
                "beneficiary",
            )?,
            "beneficiary",
        )?;
        let mut c = KeeperConfig::new(program, season, beneficiary);
        if let Some(v) = take("rpc") {
            c.rpc = strs(v, "rpc")?;
        }
        if let Some(v) = take("drand") {
            c.drand = strs(v, "drand")?;
        }
        if let Some(v) = take("roles") {
            c.roles = strs(v, "roles")?;
            if let Some(bad) = c.roles.iter().find(|r| !ROLES.contains(&r.as_str())) {
                return Err(format!("unknown role `{bad}`"));
            }
        }
        if let Some(v) = take("regions") {
            c.regions = parse_regions(&s(v, "regions")?)?;
        }
        macro_rules! num {
            ($k:literal, $f:ident, $t:ty) => {
                if let Some(v) = take($k) {
                    c.$f = int(v, $k)? as $t;
                }
            };
        }
        num!("reveal_pool", reveal_pool, usize);
        num!("delay_pool", delay_pool, usize);
        num!("funders", funders, usize);
        num!("r99_reveals", r99_reveals, u64);
        num!("p_def_milli", p_def_milli, u64);
        num!("p_delay_milli", p_delay_milli, u64);
        num!("p_low_milli", p_low_milli, u64);
        num!("delay_floor", delay_floor, u64);
        num!("daily_budget_lamports", daily_budget_lamports, u64);
        num!("per_write_cap_lamports", per_write_cap_lamports, u64);
        num!("fallback_lag_slots", fallback_lag_slots, u64);
        num!("contested_slots", contested_slots, u64);
        num!("nonce_switch_slots", nonce_switch_slots, u64);
        num!("rescan_bells", rescan_bells, u32);
        num!("care_every_slots", care_every_slots, u64);
        if let Some(v) = take("reveal_floor") {
            c.reveal_floor = Some(int(v, "reveal_floor")?);
        }
        if let Some(v) = take("daily_budget_sol") {
            c.daily_budget_lamports = match v {
                Value::Int(i) if i >= 0 => i as u64 * 1_000_000_000,
                Value::Float(f) if f >= 0.0 => (f * 1e9) as u64,
                _ => return Err("`daily_budget_sol` must be a number".into()),
            };
        }
        if let Some(v) = take("peace_start") {
            c.peace_start = Some(match v {
                Value::Float(f) if (0.0..=1.0).contains(&f) => f,
                Value::Int(i) if (0..=1).contains(&i) => i as f64,
                _ => return Err("`peace_start` must be a fraction in [0, 1]".into()),
            });
        }
        if let Some(v) = take("dev") {
            c.dev = matches!(v, Value::Bool(true));
        }
        if let Some(v) = take("api") {
            let a: SocketAddr = s(v, "api")?
                .parse()
                .map_err(|_| "`api` must be host:port".to_string())?;
            if !a.ip().is_loopback() {
                return Err("`api` must bind a loopback address (§8.2)".into());
            }
            c.api = Some(a);
        }
        for (k, f) in [
            ("token_file", &mut c.token_file),
            ("master_seed_file", &mut c.master_seed_file),
            ("journal", &mut c.journal),
            ("budgets_file", &mut c.budgets_file),
        ] {
            if let Some(v) = take(k) {
                *f = Some(PathBuf::from(s(v, k)?));
            }
        }
        if c.p_def_milli > crate::P_DEF_MILLI || c.p_delay_milli > c.p_def_milli {
            return Err("p_def_milli ≤ 2000 and p_delay_milli ≤ p_def_milli".into());
        }
        if let Some(k) = m.keys().next() {
            return Err(format!("unknown key `{k}`"));
        }
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_file() {
        let p = Address::new_from_array([5; 32]);
        let b = Address::new_from_array([6; 32]);
        let text = format!(
            r#"
            # keeper A (operator roles)
            program = "{p}"
            season = 7
            rpc = ["http://127.0.0.1:41010", "http://127.0.0.1:41510"]
            drand = ["http://127.0.0.1:41020"]
            roles = ["beacon", "archive", "rings"]
            regions = "0-3, 9"
            reveal_pool = 150
            delay_pool = 32
            funders = 4
            r99_reveals = 4_000
            p_def_milli = 2000
            p_delay_milli = 500
            daily_budget_sol = 2.5
            peace_start = 0.25
            beneficiary = "{b}"
            api = "127.0.0.1:41050"
            token_file = "k.token"  # loopback bearer token
            "#
        );
        let c = KeeperConfig::from_toml(&text).unwrap();
        assert_eq!(c.season_id, 7);
        assert_eq!(c.rpc.len(), 2);
        assert_eq!(c.regions, vec![0, 1, 2, 3, 9]);
        assert_eq!(c.daily_budget_lamports, 2_500_000_000);
        assert_eq!(c.peace_start, Some(0.25));
        assert_eq!(c.api.unwrap().port(), 41_050);
        assert!(c.has_role("archive") && !c.has_role("reveal"));
        assert!(KeeperConfig::from_toml(&text.replace("regions", "regoins")).is_err());
        assert!(
            KeeperConfig::from_toml(&text.replace("127.0.0.1:41050", "0.0.0.0:41050")).is_err()
        );
        assert!(KeeperConfig::from_toml(&text.replace("\"rings\"]", "\"ringz\"]")).is_err());
        assert!(parse_regions("15-16").is_err());
    }
}
