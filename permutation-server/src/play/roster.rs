//! The operator's AI members (V5 §18.2–§18.4) as this server knows them:
//! which members it runs, and each AI's salt, from which its home city
//! follows once `ai_home_tick` has passed. Nothing here is served while the
//! season is played except what becomes public: an AI whose home city was
//! conquered is announced with its salt (anyone can check it against the
//! member's registration tag), and the rest after the season.
//!
//! The planner never reads home cities: an AI that defended its home would
//! give itself away.

use permutation_rules::gov::MemberId;
use permutation_rules::roster::home_city;
use permutation_rules::state::{CityId, CivId, WorldState};
use serde_json::{json, Value};

use crate::codec::{from_hex, hex};

#[derive(Clone, Debug)]
pub struct AiEntry {
    pub member: MemberId,
    pub civ: CivId,
    pub salt: [u8; 32],
    /// Its home city was conquered: (city, captor, tick, bounty paid).
    pub fallen: Option<(CityId, CivId, u16, bool)>,
}

#[derive(Clone, Debug, Default)]
pub struct AiRoster {
    pub entries: Vec<AiEntry>,
    pub bounty_each: u64,
}

impl AiRoster {
    /// From the gateway's `/operator/roster` (`ai: [{member, civ, salt}]`).
    pub fn from_operator(v: &Value) -> AiRoster {
        let entries = v["ai"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                let salt: [u8; 32] = from_hex(a["salt"].as_str()?).try_into().ok()?;
                Some(AiEntry {
                    member: a["member"].as_u64()? as MemberId,
                    civ: a["civ"].as_u64()? as CivId,
                    salt,
                    fallen: None,
                })
            })
            .collect();
        AiRoster {
            entries,
            bounty_each: v["bountyEach"]
                .as_str()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0),
        }
    }

    /// Keep what was already announced when the roster is fetched again.
    pub fn merge(&mut self, fresh: AiRoster) {
        let old = std::mem::take(&mut self.entries);
        self.bounty_each = fresh.bounty_each;
        self.entries = fresh
            .entries
            .into_iter()
            .map(|mut e| {
                e.fallen = old
                    .iter()
                    .find(|o| o.member == e.member)
                    .and_then(|o| o.fallen);
                e
            })
            .collect();
    }

    /// AIs whose home city has just been conquered (first conquest after the
    /// home tick, V5 §18.3); marks them.
    pub fn check(&mut self, state: &WorldState) -> Vec<AiEntry> {
        let mut out = Vec::new();
        for e in self.entries.iter_mut().filter(|e| e.fallen.is_none()) {
            let Some(city) = home_city(state, e.civ, &e.salt) else {
                continue;
            };
            if let Some(q) = state
                .cities
                .get(city as usize)
                .and_then(|c| c.first_conquest)
            {
                e.fallen = Some((city, q.by, q.tick, q.bounty));
                out.push(e.clone());
            }
        }
        out
    }

    /// What may be shown: the AIs announced so far (all of them once the
    /// season is over), with their salts.
    pub fn public_json(&self, over: bool, names: &dyn Fn(MemberId) -> String) -> Value {
        json!({
            "aiCount": self.entries.len(),
            "bountyEach": self.bounty_each.to_string(),
            "revealed": over,
            "fallen": self.entries.iter().filter(|e| e.fallen.is_some() || over).map(|e| {
                let f = e.fallen;
                json!({
                    "member": e.member, "name": names(e.member), "civ": e.civ, "salt": hex(&e.salt),
                    "home": f.map(|x| x.0), "captor": f.map(|x| x.1), "tick": f.map(|x| x.2),
                    "bounty": f.map(|x| x.3),
                })
            }).collect::<Vec<_>>(),
        })
    }
}

// ------------------------------------------------------------------ talk

use super::game::{Game, Host};

/// Answers per poll, so a flood of messages cannot make the AIs spam back.
const REPLIES_PER_POLL: usize = 3;

impl Game {
    /// The AI members' answers to new messages addressed to them or to their
    /// nation (V5 §18.8): by the nation's temperament, pointing at what binds
    /// (a contract), never at who is an AI. `/talk` bodies; the gateway
    /// signs them (and may phrase them with a language model, `draft`).
    pub fn ai_replies(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        let pending: Vec<Value> = self
            .talk
            .iter()
            .filter(|m| {
                m["id"]
                    .as_u64()
                    .is_some_and(|id| !self.talk_answered.contains(&id))
            })
            .cloned()
            .collect();
        for m in pending {
            let id = m["id"].as_u64().unwrap_or(0);
            self.talk_answered.insert(id);
            if out.len() >= REPLIES_PER_POLL {
                continue;
            }
            let from = m["member"].as_u64().map(|x| x as MemberId);
            let is_ai = |x: MemberId| {
                self.members
                    .get(x as usize)
                    .is_some_and(|y| y.host == Host::Ai)
            };
            if from.is_none_or(is_ai) {
                continue;
            }
            // Who answers: the AI addressed, or the nation's first AI member.
            let responder = if let Some(x) = m["to"]["member"].as_u64() {
                Some(x as MemberId).filter(|x| is_ai(*x))
            } else if let Some(c) = m["to"]["civ"].as_u64() {
                (0..self.members.len() as MemberId)
                    .find(|x| self.members[*x as usize].civ as u64 == c && is_ai(*x))
            } else {
                None
            };
            let Some(r) = responder else { continue };
            let civ = self.members[r as usize].civ;
            let Some(t) = self.planner.bots.get(civ as usize).map(|b| b.traits) else {
                continue;
            };
            if t.openness < 20 {
                continue;
            }
            let price = t.price() as f64 / 1_000_000.0;
            let text = if t.aggression >= 70 {
                format!("言葉だけでは動かない。本気なら国庫の契約で {price:.1} USDC 以上を示してほしい。")
            } else if t.loyalty >= 60 {
                format!("約束は守る。条件は契約で示してほしい（{price:.1} USDC から検討する）。")
            } else {
                format!("条件次第だ。契約（{price:.1} USDC 以上）なら応じる。")
            };
            out.push(json!({
                "member": r, "to": {"member": from}, "text": text,
                "draft": {"incoming": m["text"], "aggression": t.aggression, "greed": t.greed,
                          "loyalty": t.loyalty, "openness": t.openness, "price": price},
            }));
        }
        out
    }
}
