//! Nations and governance (V5): members, offices, achievements, payouts.

use permutation_rules::gov::{MemberId, Role, NOBODY};
use permutation_rules::orders::Order;
use permutation_rules::state::{CivId, WorldState};
use permutation_rules::Ruleset;
use serde::Serialize;
use serde_json::{json, Value};

use super::dto::OrderDto;
use super::name;

/// Display data the server keeps for each member (not part of the rules).
#[derive(Clone, Debug, Default, Serialize)]
pub struct MemberMeta {
    pub name: String,
    /// "human", "agent" or "undeclared" (self-declared, V5 D16).
    pub kind: String,
    /// Verified agent registration (e.g. ERC-8004), shown as a badge.
    pub attested: bool,
}

/// The government of `civ` (V5 §5): offices, the coming election, recalls
/// and proposals. Everything here is recorded on chain, so it is public to
/// the nation; other nations' proposals are public too (perfect information).
pub fn gov_view(s: &WorldState, rules: &Ruleset, civ: CivId, meta: &[MemberMeta]) -> Value {
    let n = &s.nations[civ as usize];
    let who = |m: MemberId| {
        if m == NOBODY {
            return Value::Null;
        }
        let x = meta.get(m as usize).cloned().unwrap_or_default();
        json!({"id": m, "name": x.name, "kind": x.kind, "attested": x.attested})
    };
    let t = s.tick;
    let offices: Vec<Value> = Role::ALL
        .iter()
        .map(|r| {
            let i = r.index();
            let last = n.office_last_act[i];
            json!({
                "role": name(r), "holder": who(n.offices[i]), "since": n.office_since[i],
                "lastAct": if last == permutation_rules::gov::NEVER { Value::Null } else { json!(last) },
                "active": permutation_rules::gov::active_officer(s, rules, civ, *r).is_some(),
                "runnerUp": who(n.runner_up[i]),
            })
        })
        .collect();
    let tally = permutation_rules::gov::tally(s);
    let candidates: Vec<Value> = Role::ALL
        .iter()
        .map(|r| {
            let list: Vec<Value> = s
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.civ == civ && m.standing_for & r.bit() != 0)
                .map(|(id, _)| {
                    let votes = tally[id][r.index()];
                    json!({"member": who(id as MemberId), "votes": votes})
                })
                .collect();
            json!({"role": name(r), "candidates": list})
        })
        .collect();
    let recalls: Vec<Value> = n
        .recalls
        .iter()
        .map(|rc| json!({"role": name(rc.role), "holder": who(rc.holder), "opened": rc.opened, "automatic": rc.automatic,
                        "yes": rc.yes.len(), "closes": rc.opened + rules.recall_ticks}))
        .collect();
    let proposals: Vec<Value> = n
        .proposals
        .iter()
        .map(|p| json!({
            "id": p.id, "role": name(p.role), "proposer": who(p.proposer), "tick": p.tick,
            "expires": p.tick + rules.proposal_ttl_ticks, "supporters": p.supporters.len(),
            "supportedBy": p.supporters, "orders": p.orders.iter().map(OrderDto::from_order).collect::<Vec<_>>(),
        }))
        .collect();
    let electorate = s
        .members
        .iter()
        .filter(|m| {
            m.civ == civ
                && m.last_active != permutation_rules::gov::NEVER
                && t - m.last_active < rules.recall_electorate_ticks
        })
        .count();
    let next = permutation_rules::gov::next_term_start(rules, t);
    json!({
        "offices": offices, "candidates": candidates, "recalls": recalls, "proposals": proposals,
        "members": n.members, "electorate": electorate,
        "nextElection": next, "voteOpen": permutation_rules::gov::vote_open(rules, t),
        "voteFrom": next.map(|x| x.saturating_sub(rules.vote_window)),
        "idleRecallTicks": rules.idle_recall_ticks, "termTicks": rules.term_ticks,
    })
}

/// Milestone board of every nation (V5 §6): tiers, era and points now
/// ("if the season ended now"). Tiers and eras are public announcements.
pub fn achievements_view(s: &WorldState, rules: &Ruleset) -> Value {
    let facts = permutation_rules::scoring::facts_all(s, rules);
    let list: Vec<Value> = facts
        .iter()
        .enumerate()
        .map(|(civ, f)| {
            let sc = permutation_rules::scoring::score_of(rules, f);
            json!({
                "civ": civ, "tiers": sc.tiers, "era": sc.era, "pathPoints": sc.path_points,
                "eraPoints": sc.era_points, "points": sc.total(),
            })
        })
        .collect();
    json!({
        "nations": list,
        "tierPoints": rules.tier_points,
        "thresholds": {
            "hegemonyTiles": rules.hegemony_tiles, "hegemonyCities": rules.hegemony_cities,
            "prosperityPop": rules.prosperity_pop, "prosperityWealth": rules.prosperity_wealth,
            "scienceTechs": rules.science_techs, "concordTrade": rules.concord_trade,
        },
    })
}

/// What `civ` knows of its own milestone facts (V5 §6.2), for the board.
pub fn facts_view(s: &WorldState, rules: &Ruleset, civ: CivId) -> Value {
    let f = permutation_rules::scoring::facts_all(s, rules)[civ as usize];
    json!({
        "tiles": f.tiles, "capturedHeld": f.captured_held, "pop": f.pop, "wealth": f.wealth,
        "techs": f.techs, "starGateMax": f.star_gate_max, "partners": f.partners, "alliances": f.alliances,
        "suzerainties": f.suzerainties, "everSuzerain": f.ever_suzerain, "envoySent": f.envoy_sent, "trade": f.trade,
    })
}

/// The payout projection for the whole world (V5 §7): per nation, and per member.
pub fn settlement_view(p: &permutation_rules::payout::Settlement) -> Value {
    json!({
        "pool": p.pool, "refund": p.refund, "counted": p.counted,
        "nationShare": p.nation_share, "equalEach": p.equal_each, "perMember": p.per_member,
        "points": p.scores.iter().map(|x| x.total()).collect::<Vec<_>>(),
    })
}

/// One member as seen by itself: merit, activity, offices, projection.
pub fn member_view(
    s: &WorldState,
    rules: &Ruleset,
    m: MemberId,
    meta: &[MemberMeta],
    projection: Option<&permutation_rules::payout::Settlement>,
) -> Value {
    let Some(x) = s.members.get(m as usize) else {
        return Value::Null;
    };
    let n = &s.nations[x.civ as usize];
    let offices: Vec<String> = Role::ALL
        .iter()
        .filter(|r| n.holder(**r) == m)
        .map(name)
        .collect();
    let log: Vec<Value> = s
        .merit_log
        .iter()
        .filter(|e| e.member == m)
        .map(|e| json!({"path": name(e.path), "merit": e.milli as f64 / 1000.0, "what": String::from_utf8_lossy(e.what)}))
        .collect();
    let info = meta.get(m as usize).cloned().unwrap_or_default();
    json!({
        "id": m, "civ": x.civ, "name": info.name, "kind": info.kind, "attested": info.attested,
        "offices": offices,
        "standingFor": Role::ALL.iter().filter(|r| x.standing_for & r.bit() != 0).map(name).collect::<Vec<_>>(),
        "merit": {
            "hegemony": x.merit[0] as f64 / 1000.0, "prosperity": x.merit[1] as f64 / 1000.0,
            "science": x.merit[2] as f64 / 1000.0, "concord": x.merit[3] as f64 / 1000.0,
            "common": x.merit[4] as f64 / 1000.0, "total": x.merit_total() as f64 / 1000.0,
        },
        "activeWindows": x.active_windows(), "windowsNeeded": rules.active_windows_needed,
        "windows": rules.activity_windows(), "active": permutation_rules::gov::is_active_member(rules, x),
        "lastActive": if x.last_active == permutation_rules::gov::NEVER { Value::Null } else { json!(x.last_active) },
        "projectedPayout": projection.and_then(|p| p.per_member.get(m as usize)).copied(),
        "meritLog": log,
    })
}

/// Everyone in `civ`'s nation, for the plaza.
pub fn roster_view(s: &WorldState, rules: &Ruleset, civ: CivId, meta: &[MemberMeta]) -> Value {
    let list: Vec<Value> = s
        .members
        .iter()
        .enumerate()
        .filter(|(_, x)| x.civ == civ)
        .map(|(id, x)| {
            let info = meta.get(id).cloned().unwrap_or_default();
            json!({
                "id": id, "name": info.name, "kind": info.kind, "attested": info.attested,
                "merit": x.merit_total() as f64 / 1000.0, "active": permutation_rules::gov::is_active_member(rules, x),
                "activeWindows": x.active_windows(),
            })
        })
        .collect();
    json!(list)
}

/// This nation's orders that did not take effect last tick (v0.2 C12).
pub fn skipped_view(s: &WorldState, civ: CivId) -> Value {
    let list: Vec<Value> = s
        .last_skipped
        .iter()
        .filter(|k| k.civ == civ)
        .map(|k| json!({
            "role": Role::from_index(k.role as usize).map(name),
            "index": if k.index == u16::MAX { Value::Null } else { json!(k.index) },
            "reason": permutation_rules::checks::BLOCKED_NAMES.get(k.reason as usize).copied().unwrap_or("Unknown"),
        }))
        .collect();
    json!(list)
}

/// The office an order belongs to, by name (for clients' drafting).
pub fn office_of(s: &WorldState, o: &Order) -> Option<String> {
    Role::ALL
        .into_iter()
        .find(|r| permutation_rules::orders::role_allows(s, *r, o))
        .map(name)
}
