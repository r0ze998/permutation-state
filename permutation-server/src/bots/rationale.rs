//! The rationale a bot seals with its orders (§4.3): what it did and why,
//! in Japanese, from the same state the orders came from.

use permutation_rules::buildings::Building;
use permutation_rules::orders::{AttackTarget, Order, StandingOrder};
use permutation_rules::state::{CivId, QueueItem, WorldState};
use permutation_rules::tech::Tech;
use permutation_rules::units::UnitType;

use super::geo::troops_of;
use super::Bot;

/// Nation names as the rationale writes them (the web client's `CIV_NAMES`
/// in `web/i18n.mjs` holds the same). The text feeds the sealed decision
/// digests, so it must not change.
const NAMES_JA: [&str; 6] = [
    "アステル",
    "ボレアリス",
    "シンダー",
    "ダンマール",
    "エンバー",
    "フィヨルダル",
];

fn civ_ja(c: CivId) -> &'static str {
    NAMES_JA.get(c as usize).copied().unwrap_or("?")
}

// Exhaustive matches, so a new tech, building or unit does not compile
// until it has a name here.
fn tech_ja(t: Tech) -> &'static str {
    use Tech::*;
    match t {
        Agriculture => "農業",
        BronzeWorking => "青銅器",
        Archery => "弓術",
        HorsebackRiding => "騎乗",
        Masonry => "石工",
        Mysticism => "神秘主義",
        Writing => "筆記",
        Currency => "通貨",
        IronWorking => "製鉄",
        Mathematics => "数学",
        Chivalry => "騎士道",
        Philosophy => "哲学",
        Engineering => "工学",
        Astronomy => "天文学",
        Physics => "物理学",
        CelestialMechanics => "天体力学",
    }
}

fn building_ja(b: Building) -> &'static str {
    use Building::*;
    match b {
        Granary => "穀物庫",
        Workshop => "工房",
        Temple => "神殿",
        Market => "市場",
        Academy => "学術院",
        Barracks => "兵舎",
        Walls => "城壁",
        StarGate1 => "スターゲートI",
        StarGate2 => "スターゲートII",
        StarGate3 => "スターゲートIII",
    }
}

fn unit_ja(u: UnitType) -> &'static str {
    use UnitType::*;
    match u {
        Spearman => "槍兵",
        Archer => "弓兵",
        Horseman => "騎兵",
        Pikeman => "長槍兵",
        Crossbowman => "弩兵",
        Knight => "騎士",
        Scout => "斥候",
        Settler => "開拓者",
    }
}

fn item_ja(i: &QueueItem) -> String {
    match i {
        QueueItem::Building(b) => building_ja(*b).to_string(),
        QueueItem::Troops { unit, n } => format!("{}×{}", unit_ja(*unit), n),
        QueueItem::Scout => "斥候".to_string(),
        QueueItem::Settler => "開拓者".to_string(),
    }
}

impl Bot {
    /// A short, honest summary of why these orders were chosen, written from
    /// the same state the orders came from. Committed before the tick
    /// resolves and revealed after it.
    pub fn rationale(&self, s: &WorldState, orders: &[Order]) -> String {
        let civ = self.civ;
        let mine = troops_of(s, civ);
        let mut parts: Vec<String> = Vec::new();
        let mut moves = 0;
        let mut scouting = 0;
        for o in orders {
            match o {
                Order::DeclareWar { civ: t } => parts.push(format!(
                    "{}に宣戦：見えている首都で最も近く、自軍{}に対し目視の相手兵{}",
                    civ_ja(*t),
                    mine,
                    troops_of(s, *t)
                )),
                Order::ProposePeace { civ: t } => {
                    parts.push(format!("{}に講和を提案：戦争を長引かせない", civ_ja(*t)))
                }
                Order::AcceptPeace { civ: t } => parts.push(format!("{}の講和を受諾", civ_ja(*t))),
                Order::AcceptNap { civ: t, .. } => {
                    parts.push(format!("{}の不可侵条約を受諾：金に余裕あり", civ_ja(*t)))
                }
                Order::ProposeNap { civ: t, .. } => parts.push(format!(
                    "{}に不可侵条約を提案：国境を落ち着かせる",
                    civ_ja(*t)
                )),
                Order::ProposeAlliance { civ: t } => {
                    parts.push(format!("{}に同盟を提案：同じ外交方針", civ_ja(*t)))
                }
                Order::AcceptAlliance { civ: t } => {
                    parts.push(format!("{}の同盟に参加", civ_ja(*t)))
                }
                Order::Attack { target, .. } => parts.push(match target {
                    AttackTarget::City(id) => format!("都市{}を攻撃：射程内の敵都市を優先", id),
                    AttackTarget::Unit(_) => "射程内で最も弱い敵部隊を攻撃".to_string(),
                    AttackTarget::CityState(id) => format!("都市国家{}を攻撃", id + 1),
                }),
                Order::FoundCity { .. } => {
                    parts.push("開拓者が良い立地に着いたので都市を建設".to_string())
                }
                Order::SetResearch { techs } => parts.push(format!(
                    "研究：{}",
                    techs
                        .iter()
                        .map(|t| tech_ja(*t))
                        .collect::<Vec<_>>()
                        .join("→")
                )),
                Order::SetQueue { city, items } => parts.push(format!(
                    "都市{}の生産：{}",
                    city,
                    if items.is_empty() {
                        "なし".to_string()
                    } else {
                        items.iter().map(item_ja).collect::<Vec<_>>().join("→")
                    }
                )),
                Order::SendEnvoy { city_state, .. } => {
                    parts.push(format!("最寄りの都市国家{}へ使節", city_state + 1))
                }
                Order::Purchase { .. } => parts.push("余った金で首都の生産を購入".to_string()),
                Order::SetStanding { rule, .. } => parts.push(match rule {
                    StandingOrder::AutoDefend { radius } => {
                        format!("首都の守備隊に自動防衛（半径{radius}）")
                    }
                    StandingOrder::Retreat { .. } => {
                        "野戦軍に撤退ルール：1.5倍の敵で後退".to_string()
                    }
                    StandingOrder::AutoPurchase { max_gold } => {
                        format!("首都で毎ティック{max_gold}金まで自動購入")
                    }
                    _ => "継続命令を更新".to_string(),
                }),
                Order::MoveUnit { unit, .. } => {
                    if s.units
                        .get(*unit as usize)
                        .is_some_and(|u| u.unit_type == UnitType::Scout)
                    {
                        scouting += 1;
                    } else {
                        moves += 1;
                    }
                }
                _ => {}
            }
        }
        if scouting > 0 {
            parts.push("斥候で未踏の地を探索".to_string());
        }
        if moves > 0 {
            parts.push(format!("部隊{}つを移動（開拓地・進軍）", moves));
        }
        if parts.is_empty() {
            parts.push("様子見：急ぐ判断なし".to_string());
        }
        format!("[{}] {}", self.persona.name(), parts.join(" / "))
    }
}
