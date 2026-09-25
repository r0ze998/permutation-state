//! Members' messages (V5 §18.7): kept here in local mode, relayed through
//! the gateway (which signs and anchors them) in chain mode; and the hosted
//! AI members' answers.

use permutation_rules::gov::MemberId;
use serde_json::{json, Value};

use super::game::{Game, Host};

/// Longest message, in characters.
pub const MAX_TALK_CHARS: usize = 280;
/// Messages kept in memory (chain mode; the gateway keeps them all).
const KEEP_TALK: usize = 500;

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

impl Game {
    /// Keep a message (local mode); returns its id.
    pub fn push_talk(&mut self, member: Value, to: Value, text: Value) -> u64 {
        let id = self.talk_next;
        self.talk.push(
            json!({"id": id, "tick": self.state.tick, "member": member, "to": to, "text": text}),
        );
        self.talk_next += 1;
        id
    }

    /// Messages the gateway relayed since the last poll (chain mode).
    pub fn receive_talk(&mut self, talk: Vec<Value>) {
        if talk.is_empty() {
            return;
        }
        self.talk_next = talk
            .iter()
            .filter_map(|m| m["id"].as_u64())
            .max()
            .map_or(self.talk_next, |x| x + 1);
        self.talk.extend(talk);
        let cut = self.talk.len().saturating_sub(KEEP_TALK);
        self.talk.drain(..cut);
    }
}
