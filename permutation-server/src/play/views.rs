//! What `/api/state` and `/api/lobby` answer. Everything a viewer sees of the
//! world is the full state for everyone (perfect information, `fog`).
//!
//! Nothing public tells the operator's AI members apart (V5 §18.2): no host,
//! no "ready", no kind in a season with AI members, and any member's public
//! block (`?member=M`) has the same fields whoever runs it.

use permutation_rules::gov::{MemberId, Role};
use permutation_rules::tech::TECHS;
use permutation_rules::Ruleset;
use serde_json::{json, Value};
use std::time::Instant;

use super::chain::number;
use super::game::{role_name, unix_now, Game, Host, Phase, Viewer};
use crate::api;
use crate::codec::hex;

/// Chronicle lines served per request.
const CHRONICLE_SERVED: usize = 150;

/// Rule numbers the client shows, so it keeps no copy of them.
fn rules_json(r: &Ruleset) -> Value {
    json!({
        "napMinBond": r.nap_min_bond, "napTicks": r.nap_ticks,
        "suzerainThreshold": r.suzerain_threshold, "suzerainLockTicks": r.suzerain_lock_ticks,
        "exchangeFeeBps": r.exchange_fee_bps, "ammFeeBps": r.amm_fee_bps, "hubFeeBps": r.hub_fee_bps,
        "maxOfficesPerMember": r.max_offices_per_member, "bankTicks": r.bank_ticks,
        "budgetBase": r.budget_base, "budgetCap": r.budget_cap,
        "casusBelliThreshold": r.casus_belli_threshold, "proposalTtl": r.proposal_ttl,
        "truceTicks": r.truce_ticks, "allianceLeaveDelay": r.alliance_leave_delay,
        "recallTicks": r.recall_ticks, "recallElectorateTicks": r.recall_electorate_ticks,
        "techCount": TECHS.len(),
    })
}

/// `/api/lobby` in chain mode before the government opened: `registering`
/// (no world yet: people register through the gateway) or `starting` (the
/// world is being set up). `info` is the gateway's `/season`, if known: what
/// the browser checks the gateway's payment request against.
pub fn pending_lobby(phase: &str, gateway: &str, info: Option<&Value>) -> Value {
    let chain = info.map(|i| {
        json!({
            "seasonId": number(&i["season"]["seasonId"]).map(|x| x.to_string()),
            "programId": i["programId"], "cluster": i["cluster"], "accounts": i["accounts"],
        })
    });
    let fee = info.and_then(|i| number(&i["season"]["entryFee"]));
    json!({"phase": phase, "mode": "chain", "gateway": gateway, "entryFee": fee, "chain": chain})
}

impl Game {
    pub fn members_json(&self) -> Value {
        let hide = self.hides_kinds();
        json!(self
            .members
            .iter()
            .enumerate()
            // Never who hosts a member, nor whether it ended its turn (only
            // people did): either would tell observers who the operator's
            // AI members are (V5 §18.2). Nor, in a season with AI members,
            // any self-declared kind.
            .map(|(i, m)| {
                let mut v = json!({"id": i, "civ": m.civ, "name": m.meta.name});
                if !hide {
                    v["kind"] = json!(m.meta.kind);
                    v["attested"] = json!(m.meta.attested);
                }
                v
            })
            .collect::<Vec<_>>())
    }

    pub fn lobby_json(&self, viewer: Viewer) -> Value {
        let p = self.projection();
        let nations: Vec<Value> = self
            .state
            .civs
            .iter()
            .enumerate()
            .map(|(c, x)| {
                let members = self.members.iter().filter(|m| m.civ as usize == c).count();
                let share = p.nation_share.get(c).copied();
                json!({"civ": c, "name": x.name, "members": members,
                       "era": x.achievements.era, "points": p.scores.get(c).map(|s| s.total()),
                       "share": share, "perMember": if members == 0 { Value::Null } else { json!(share.unwrap_or(0) / members as u64) }})
            })
            .collect();
        let you = match viewer {
            Viewer::Member(m) => {
                let x = &self.members[m as usize];
                json!({"id": m, "civ": x.civ, "name": x.meta.name,
                       "stand": Role::ALL.iter().filter(|r| x.stand & r.bit() != 0).map(|r| role_name(*r)).collect::<Vec<_>>(),
                       "votes": Role::ALL.iter().map(|r| api::member_ref(x.votes[r.index()])).collect::<Vec<_>>(), "deposit": x.deposit})
            }
            _ => Value::Null,
        };
        let mut v = json!({
            "phase": self.phase.name(),
            "mode": if self.chain.is_some() { "chain" } else { "local" },
            "nations": nations, "members": self.members_json(), "you": you,
            "entryFee": self.entry_fee, "pool": self.pool(), "market": self.rules.market_enabled,
            "opsShareBps": self.rules.ops_share_bps, "maxOfficesPerMember": self.rules.max_offices_per_member,
            "offices": Role::ALL.iter().map(|r| role_name(*r)).collect::<Vec<_>>(),
            "roster": self.roster_json(),
        });
        if let Some(c) = &self.chain {
            v["gateway"] = json!(c.gateway);
            v["chain"] = self.chain_json().unwrap_or_default();
        }
        v
    }

    /// The operator's AI members as far as they are public (V5 §18.2).
    pub fn roster_json(&self) -> Value {
        let names = |m: MemberId| {
            self.members
                .get(m as usize)
                .map_or_else(|| format!("member {m}"), |x| x.meta.name.clone())
        };
        let over = self.state.tick >= self.rules.ticks_per_season;
        let mut v = self.roster.public_json(over, &names);
        v["homeTick"] = json!(self.rules.ai_home_tick);
        v
    }

    /// Seconds left to commit orders in the open tick (chain mode: 0 once
    /// the commitments closed).
    fn seconds_left(&self) -> f64 {
        if let Some(c) = &self.chain {
            match c.tick_phase() {
                "commit" => (c.meta.deadline - unix_now()).max(0) as f64,
                _ => 0.0,
            }
        } else if self.paused || self.phase == Phase::Lobby {
            self.tick_seconds as f64
        } else {
            self.deadline
                .saturating_duration_since(Instant::now())
                .as_secs_f64()
        }
    }

    fn season_json(&self) -> Value {
        let r = &self.rules;
        json!({
            "preset": format!("{:?}", r.preset), "ticks": r.ticks_per_season,
            "entryFee": self.entry_fee, "pool": self.pool(), "market": r.market_enabled,
            "marketFreeze": r.exchange_freeze_tick, "transferFreeze": r.transfer_freeze_tick,
            "termTicks": r.term_ticks, "voteWindow": r.vote_window,
            "phases": [0, r.expansion_start, r.contention_start, r.crisis_start, r.resolution_start],
            "activityWindow": r.activity_window_ticks, "activeWindowsNeeded": r.active_windows_needed,
            "equalShareBps": r.equal_share_bps, "opsShareBps": r.ops_share_bps,
            "spendConsentUsdc": r.spend_consent_usdc, "deliveryTicks": r.delivery_ticks,
            "tariffTable": r.tariff_table_bps, "tariffFull": r.tariff_full_usdc,
            "rules": rules_json(r),
        })
    }

    /// Chain mode: the open tick's phase (`commit|reveal|frozen|finished`)
    /// and the seconds left in it (the reveal window in `reveal`).
    fn tick_phase(&self) -> (Value, f64) {
        match &self.chain {
            Some(c) => {
                let phase = c.tick_phase();
                let left = match phase {
                    "commit" | "reveal" => (c.meta.deadline - unix_now()).max(0) as f64,
                    _ => 0.0,
                };
                (json!(phase), left)
            }
            None => (Value::Null, self.seconds_left()),
        }
    }

    fn chain_json(&self) -> Option<Value> {
        let c = self.chain.as_ref()?;
        Some(json!({
            "seasonId": c.meta.season_id.to_string(),
            "programId": c.info["programId"], "cluster": c.info["cluster"],
            // Only the RPC URLs the operator published: the operator's own
            // may carry an API key (the gateway's `--public-base-rpc`).
            "accounts": c.info["accounts"], "endpoints": c.info["publicEndpoints"],
            "gateway": c.gateway, "layer": c.layer, "slot": c.slot, "phase": c.phase,
            "finished": c.meta.finished,
            "lastTick": c.last_record.as_ref().map(|r| json!({"tick": r["tick"], "signature": r["signature"], "cu": r["cu"], "root": r["root"], "preRoot": r["preRoot"]})),
        }))
    }

    pub fn state_json(&self, viewer: Viewer) -> Value {
        let civ = self.civ_of(viewer);
        let belief = civ.map(|c| self.fog.belief(&self.state, c));
        let s = belief.as_deref().unwrap_or(&self.state);
        let mut v = api::world_view(s, &self.rules, civ, &self.fog);
        // Perfect information: the whole chronicle is public (research too:
        // it is on chain).
        let chronicle: Vec<Value> = self
            .chronicle
            .iter()
            .rev()
            .take(CHRONICLE_SERVED)
            .map(|(t, e)| json!({"tick": t, "text": e}))
            .collect();
        let metas = self.metas();
        let projection = self.projection();
        let t = self.state.tick;
        let o = v.as_object_mut().expect("world_view is an object");
        o.insert("phase".into(), json!(self.phase.name()));
        o.insert(
            "viewer".into(),
            json!(match viewer {
                Viewer::Member(_) => "member",
                Viewer::Watch(..) => "watch",
                Viewer::Spectator => "spectator",
            }),
        );
        o.insert("secondsLeft".into(), json!(self.seconds_left()));
        let (tick_phase, phase_left) = self.tick_phase();
        o.insert("chainPhase".into(), tick_phase);
        o.insert("phaseSecondsLeft".into(), json!(phase_left));
        o.insert("tickSeconds".into(), json!(self.tick_seconds));
        o.insert("paused".into(), json!(self.paused));
        o.insert("over".into(), json!(self.over()));
        o.insert("chronicle".into(), json!(chronicle));
        // The operator's AI members as far as they are public, for every
        // viewer under `aiRoster`. `roster` is this nation's member list
        // when the view has a nation (below); a spectator's `roster` stays
        // the AI roster, as it always was.
        let ai_roster = self.roster_json();
        o.insert("aiRoster".into(), ai_roster.clone());
        o.insert("roster".into(), ai_roster);
        // Members' messages, public (V5 §18.7): the latest 100.
        let from = self.talk.len().saturating_sub(100);
        o.insert("talk".into(), json!(self.talk[from..]));
        o.insert(
            "lastSummary".into(),
            json!(self.last_events.iter().collect::<Vec<_>>()),
        );
        o.insert("resolvedTick".into(), json!(self.resolved_tick));
        o.insert("members".into(), self.members_json());
        o.insert(
            "achievements".into(),
            api::achievements_view(&self.state, &self.rules),
        );
        o.insert("projection".into(), api::settlement_view(&projection));
        o.insert("season".into(), self.season_json());
        if let Some(c) = civ {
            o.insert("gov".into(), api::gov_view(s, &self.rules, c, &metas));
            // The nation's members (merit, activity); the AI roster stays
            // under `aiRoster`.
            o.insert(
                "roster".into(),
                api::roster_view(&self.state, &self.rules, c, &metas),
            );
            o.insert("facts".into(), api::facts_view(&self.state, &self.rules, c));
            o.insert("skipped".into(), api::skipped_view(&self.state, c));
            // obsRoot is public: an outside agent commits its decision against it.
            o.insert(
                "decision".into(),
                json!({"tick": t, "obsRoot": self.ledger.root(t, c).map(|h| hex(&h))}),
            );
        }
        match viewer {
            Viewer::Member(m) => {
                let x = &self.members[m as usize];
                let mut mv =
                    api::member_view(&self.state, &self.rules, m, &metas, Some(&projection));
                mv["committed"] = json!(Role::ALL
                    .iter()
                    .map(|r| json!({"role": role_name(*r), "orders": x.committed[r.index()], "cost": x.cost[r.index()], "adopt": x.adopt[r.index()],
                        "digest": self.ledger.record(t, x.civ, *r as u8).map(|rec| hex(&rec.digest))}))
                    .collect::<Vec<_>>());
                mv["ready"] = json!(x.ready == Some(t));
                mv["host"] = json!(x.host.name());
                o.insert("member".into(), mv);
                if self.chain.is_none() {
                    // People still playing this tick (a count, never who).
                    let waiting = self
                        .members
                        .iter()
                        .enumerate()
                        .filter(|(i, y)| {
                            *i != m as usize
                                && y.host == Host::Human
                                && y.here()
                                && y.ready != Some(t)
                        })
                        .count();
                    o.insert("waiting".into(), json!(waiting));
                }
            }
            // `?member=M`: that member's public block, the same fields for
            // every member (nothing this server keeps for it alone).
            Viewer::Watch(_, Some(m)) => {
                o.insert(
                    "member".into(),
                    api::member_view(&self.state, &self.rules, m, &metas, Some(&projection)),
                );
            }
            _ => {}
        }
        if let Some(chain) = self.chain_json() {
            o.insert("chain".into(), chain);
        }
        v
    }
}
