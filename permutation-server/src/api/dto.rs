//! Orders and governance actions as clients send them, and back.

use permutation_rules::contracts::ContractTerm;
use permutation_rules::gov::{GovAction, MemberId, Role};
use permutation_rules::hex::Hex;
use permutation_rules::orders::{AttackTarget, Good, Order, Side, StandingOrder, StandingTarget};
use permutation_rules::state::{CivId, QueueItem};
use serde::{Deserialize, Serialize};

use super::{name, parse_building, parse_focus, parse_role, parse_side, parse_tech, parse_unit};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ItemDto {
    Building { building: String },
    Troops { unit: String, n: u8 },
    Scout,
    Settler,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TargetDto {
    Unit { id: u32 },
    City { id: u32 },
    CityState { id: u16 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GoodDto {
    Gold,
    Iron,
    Horses,
    Food { city: u32 },
    Production { city: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum OrderDto {
    MoveUnit {
        unit: u32,
        path: Vec<[i32; 2]>,
    },
    Attack {
        army: u32,
        target: TargetDto,
    },
    FoundCity {
        settler: u32,
    },
    SetQueue {
        city: u32,
        items: Vec<ItemDto>,
    },
    SetFocus {
        city: u32,
        focus: String,
    },
    Purchase {
        city: u32,
        gold: u32,
    },
    SetResearch {
        techs: Vec<String>,
    },
    DeclareWar {
        civ: CivId,
    },
    ProposePeace {
        civ: CivId,
    },
    AcceptPeace {
        civ: CivId,
    },
    ProposeNap {
        civ: CivId,
        bond: u32,
    },
    AcceptNap {
        civ: CivId,
        bond: u32,
    },
    BreakNap {
        civ: CivId,
    },
    ProposeAlliance {
        civ: CivId,
    },
    AcceptAlliance {
        civ: CivId,
    },
    LeaveAlliance,
    SendEnvoy {
        city_state: u16,
        influence: u32,
    },
    Transfer {
        civ: CivId,
        good: GoodDto,
        amount: u32,
    },
    MarketTrade {
        good: GoodDto,
        side: String,
        amount: u32,
        limit_gold: u32,
    },
    ExchangeOrder {
        good: GoodDto,
        side: String,
        amount: u32,
        price: u64,
    },
    Raze {
        city: u32,
    },
    SetStanding {
        target: TargetDto,
        rule: StandingDto,
    },
    /// Opens an earlier decision commitment (§7.5). `salt` is 32 hex chars.
    RevealRationale {
        tick: u16,
        policy: String,
        salt: String,
        text: String,
    },
    /// The general or steward agrees to this tick's war on `civ` (V5 §5.6).
    ConsentWar {
        civ: CivId,
    },
    /// Another officer allows treasury spending up to `usdc` this tick (V5 §7.5).
    ConsentSpend {
        usdc: u64,
    },
    /// Escrow treasury USDC for a contract (V5 §18.6). `to` is omitted for
    /// an open `Capture` offer.
    OfferContract {
        #[serde(default)]
        to: Option<CivId>,
        term: TermDto,
        usdc: u64,
        deadline: u16,
    },
    AcceptContract {
        id: u32,
    },
    CancelContract {
        id: u32,
    },
}

/// A contract's condition (V5 §18.6) as clients send it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum TermDto {
    Peace,
    LeaveAlliance { with: CivId },
    KeepNap { every: u16, installments: u8 },
    Capture { city: u32 },
}

impl TermDto {
    pub fn term(self) -> ContractTerm {
        match self {
            TermDto::Peace => ContractTerm::Peace,
            TermDto::LeaveAlliance { with } => ContractTerm::LeaveAlliance { with },
            TermDto::KeepNap {
                every,
                installments,
            } => ContractTerm::KeepNap {
                every,
                installments,
            },
            TermDto::Capture { city } => ContractTerm::Capture { city },
        }
    }

    pub fn of(t: ContractTerm) -> TermDto {
        match t {
            ContractTerm::Peace => TermDto::Peace,
            ContractTerm::LeaveAlliance { with } => TermDto::LeaveAlliance { with },
            ContractTerm::KeepNap {
                every,
                installments,
            } => TermDto::KeepNap {
                every,
                installments,
            },
            ContractTerm::Capture { city } => TermDto::Capture { city },
        }
    }
}

/// Standing rules (§13) as the client sends them.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum StandingDto {
    Clear,
    AutoDefend { radius: u8 },
    Retreat { ratio_bps: u32 },
    Patrol { route: Vec<[i32; 2]> },
    QueueRepeat { on: bool },
    AutoPurchase { max_gold: u32 },
}

pub(super) fn good(g: &GoodDto) -> Good {
    match *g {
        GoodDto::Gold => Good::Gold,
        GoodDto::Iron => Good::Iron,
        GoodDto::Horses => Good::Horses,
        GoodDto::Food { city } => Good::Food(city),
        GoodDto::Production { city } => Good::Production(city),
    }
}

pub(super) fn item(i: &ItemDto) -> Result<QueueItem, String> {
    Ok(match i {
        ItemDto::Building { building } => QueueItem::Building(
            parse_building(building).ok_or_else(|| format!("unknown building {building}"))?,
        ),
        ItemDto::Troops { unit, n } => QueueItem::Troops {
            unit: parse_unit(unit).ok_or_else(|| format!("unknown unit {unit}"))?,
            n: *n,
        },
        ItemDto::Scout => QueueItem::Scout,
        ItemDto::Settler => QueueItem::Settler,
    })
}

pub fn item_dto(i: QueueItem) -> ItemDto {
    match i {
        QueueItem::Building(b) => ItemDto::Building { building: name(b) },
        QueueItem::Troops { unit, n } => ItemDto::Troops {
            unit: name(unit),
            n,
        },
        QueueItem::Scout => ItemDto::Scout,
        QueueItem::Settler => ItemDto::Settler,
    }
}

pub(super) fn good_dto(g: Good) -> GoodDto {
    match g {
        Good::Gold => GoodDto::Gold,
        Good::Iron => GoodDto::Iron,
        Good::Horses => GoodDto::Horses,
        Good::Food(city) => GoodDto::Food { city },
        Good::Production(city) => GoodDto::Production { city },
    }
}

pub(super) fn side_name(s: Side) -> String {
    match s {
        Side::Buy => "Buy".into(),
        Side::Sell => "Sell".into(),
    }
}

impl OrderDto {
    /// The order's `type` tag, as clients send it.
    pub fn type_name(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v["type"].as_str().map(String::from))
            .unwrap_or_default()
    }

    /// The inverse of `to_order` (used to send engine-built orders, e.g. the
    /// bots', through the gateway). `from_order(o).to_order() == o`.
    pub fn from_order(o: &Order) -> OrderDto {
        use OrderDto as D;
        let target = |t: &AttackTarget| match *t {
            AttackTarget::Unit(id) => TargetDto::Unit { id },
            AttackTarget::City(id) => TargetDto::City { id },
            AttackTarget::CityState(id) => TargetDto::CityState { id },
        };
        match o {
            Order::MoveUnit { unit, path } => D::MoveUnit {
                unit: *unit,
                path: path.iter().map(|h| [h.q, h.r]).collect(),
            },
            Order::Attack { army, target: t } => D::Attack {
                army: *army,
                target: target(t),
            },
            Order::FoundCity { settler } => D::FoundCity { settler: *settler },
            Order::SetQueue { city, items } => D::SetQueue {
                city: *city,
                items: items.iter().map(|i| item_dto(*i)).collect(),
            },
            Order::SetFocus { city, focus } => D::SetFocus {
                city: *city,
                focus: name(focus),
            },
            Order::Purchase { city, gold } => D::Purchase {
                city: *city,
                gold: *gold,
            },
            Order::SetResearch { techs } => D::SetResearch {
                techs: techs.iter().map(name).collect(),
            },
            Order::DeclareWar { civ } => D::DeclareWar { civ: *civ },
            Order::ProposePeace { civ } => D::ProposePeace { civ: *civ },
            Order::AcceptPeace { civ } => D::AcceptPeace { civ: *civ },
            Order::ProposeNap { civ, bond } => D::ProposeNap {
                civ: *civ,
                bond: *bond,
            },
            Order::AcceptNap { civ, bond } => D::AcceptNap {
                civ: *civ,
                bond: *bond,
            },
            Order::BreakNap { civ } => D::BreakNap { civ: *civ },
            Order::ProposeAlliance { civ } => D::ProposeAlliance { civ: *civ },
            Order::AcceptAlliance { civ } => D::AcceptAlliance { civ: *civ },
            Order::LeaveAlliance => D::LeaveAlliance,
            Order::SendEnvoy {
                city_state,
                influence,
            } => D::SendEnvoy {
                city_state: *city_state,
                influence: *influence,
            },
            Order::Transfer { civ, good, amount } => D::Transfer {
                civ: *civ,
                good: good_dto(*good),
                amount: *amount,
            },
            Order::MarketTrade {
                good,
                side,
                amount,
                limit_gold,
            } => D::MarketTrade {
                good: good_dto(*good),
                side: side_name(*side),
                amount: *amount,
                limit_gold: *limit_gold,
            },
            Order::ExchangeOrder {
                good,
                side,
                amount,
                price,
            } => D::ExchangeOrder {
                good: good_dto(*good),
                side: side_name(*side),
                amount: *amount,
                price: *price,
            },
            Order::Raze { city } => D::Raze { city: *city },
            Order::SetStanding { target: t, rule } => D::SetStanding {
                target: match *t {
                    StandingTarget::Unit(id) => TargetDto::Unit { id },
                    StandingTarget::City(id) => TargetDto::City { id },
                },
                rule: match rule {
                    StandingOrder::Clear => StandingDto::Clear,
                    StandingOrder::AutoDefend { radius } => {
                        StandingDto::AutoDefend { radius: *radius }
                    }
                    StandingOrder::Retreat { ratio_bps } => StandingDto::Retreat {
                        ratio_bps: *ratio_bps,
                    },
                    StandingOrder::Patrol { route } => StandingDto::Patrol {
                        route: route.iter().map(|h| [h.q, h.r]).collect(),
                    },
                    StandingOrder::QueueRepeat { on } => StandingDto::QueueRepeat { on: *on },
                    StandingOrder::AutoPurchase { max_gold } => StandingDto::AutoPurchase {
                        max_gold: *max_gold,
                    },
                },
            },
            Order::RevealRationale {
                tick,
                policy,
                salt,
                text,
            } => D::RevealRationale {
                tick: *tick,
                policy: String::from_utf8_lossy(policy).into_owned(),
                salt: salt.iter().map(|b| format!("{b:02x}")).collect(),
                text: String::from_utf8_lossy(text).into_owned(),
            },
            Order::ConsentWar { civ } => D::ConsentWar { civ: *civ },
            Order::ConsentSpend { usdc } => D::ConsentSpend { usdc: *usdc },
            Order::OfferContract {
                to,
                term,
                usdc,
                deadline,
            } => D::OfferContract {
                to: *to,
                term: TermDto::of(*term),
                usdc: *usdc,
                deadline: *deadline,
            },
            Order::AcceptContract { id } => D::AcceptContract { id: *id },
            Order::CancelContract { id } => D::CancelContract { id: *id },
        }
    }

    pub fn to_order(&self) -> Result<Order, String> {
        use OrderDto as D;
        Ok(match self {
            D::MoveUnit { unit, path } => Order::MoveUnit {
                unit: *unit,
                path: path.iter().map(|[q, r]| Hex::new(*q, *r)).collect(),
            },
            D::Attack { army, target } => Order::Attack {
                army: *army,
                target: match *target {
                    TargetDto::Unit { id } => AttackTarget::Unit(id),
                    TargetDto::City { id } => AttackTarget::City(id),
                    TargetDto::CityState { id } => AttackTarget::CityState(id),
                },
            },
            D::FoundCity { settler } => Order::FoundCity { settler: *settler },
            D::SetQueue { city, items } => Order::SetQueue {
                city: *city,
                items: items.iter().map(item).collect::<Result<_, _>>()?,
            },
            D::SetFocus { city, focus } => Order::SetFocus {
                city: *city,
                focus: parse_focus(focus).ok_or("unknown focus")?,
            },
            D::Purchase { city, gold } => Order::Purchase {
                city: *city,
                gold: *gold,
            },
            D::SetResearch { techs } => Order::SetResearch {
                techs: techs
                    .iter()
                    .map(|t| parse_tech(t).ok_or_else(|| format!("unknown tech {t}")))
                    .collect::<Result<_, _>>()?,
            },
            D::DeclareWar { civ } => Order::DeclareWar { civ: *civ },
            D::ProposePeace { civ } => Order::ProposePeace { civ: *civ },
            D::AcceptPeace { civ } => Order::AcceptPeace { civ: *civ },
            D::ProposeNap { civ, bond } => Order::ProposeNap {
                civ: *civ,
                bond: *bond,
            },
            D::AcceptNap { civ, bond } => Order::AcceptNap {
                civ: *civ,
                bond: *bond,
            },
            D::BreakNap { civ } => Order::BreakNap { civ: *civ },
            D::ProposeAlliance { civ } => Order::ProposeAlliance { civ: *civ },
            D::AcceptAlliance { civ } => Order::AcceptAlliance { civ: *civ },
            D::LeaveAlliance => Order::LeaveAlliance,
            D::SendEnvoy {
                city_state,
                influence,
            } => Order::SendEnvoy {
                city_state: *city_state,
                influence: *influence,
            },
            D::Transfer {
                civ,
                good: g,
                amount,
            } => Order::Transfer {
                civ: *civ,
                good: good(g),
                amount: *amount,
            },
            D::MarketTrade {
                good: g,
                side,
                amount,
                limit_gold,
            } => Order::MarketTrade {
                good: good(g),
                side: parse_side(side).ok_or("unknown side")?,
                amount: *amount,
                limit_gold: *limit_gold,
            },
            D::ExchangeOrder {
                good: g,
                side,
                amount,
                price,
            } => Order::ExchangeOrder {
                good: good(g),
                side: parse_side(side).ok_or("unknown side")?,
                amount: *amount,
                price: *price,
            },
            D::Raze { city } => Order::Raze { city: *city },
            D::ConsentWar { civ } => Order::ConsentWar { civ: *civ },
            D::ConsentSpend { usdc } => Order::ConsentSpend { usdc: *usdc },
            D::OfferContract {
                to,
                term,
                usdc,
                deadline,
            } => Order::OfferContract {
                to: *to,
                term: term.term(),
                usdc: *usdc,
                deadline: *deadline,
            },
            D::AcceptContract { id } => Order::AcceptContract { id: *id },
            D::CancelContract { id } => Order::CancelContract { id: *id },
            D::RevealRationale {
                tick,
                policy,
                salt,
                text,
            } => {
                let raw: Vec<u8> = (0..salt.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(salt.get(i..i + 2).unwrap_or("zz"), 16))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "salt must be hex".to_string())?;
                Order::RevealRationale {
                    tick: *tick,
                    policy: policy.as_bytes().to_vec(),
                    salt: raw
                        .try_into()
                        .map_err(|_| "salt must be 16 bytes".to_string())?,
                    text: text.as_bytes().to_vec(),
                }
            }
            D::SetStanding { target, rule } => Order::SetStanding {
                target: match *target {
                    TargetDto::Unit { id } => StandingTarget::Unit(id),
                    TargetDto::City { id } => StandingTarget::City(id),
                    TargetDto::CityState { .. } => {
                        return Err("city-states take no standing rules".into())
                    }
                },
                rule: match rule {
                    StandingDto::Clear => StandingOrder::Clear,
                    StandingDto::AutoDefend { radius } => {
                        StandingOrder::AutoDefend { radius: *radius }
                    }
                    StandingDto::Retreat { ratio_bps } => StandingOrder::Retreat {
                        ratio_bps: *ratio_bps,
                    },
                    StandingDto::Patrol { route } => StandingOrder::Patrol {
                        route: route.iter().map(|[q, r]| Hex::new(*q, *r)).collect(),
                    },
                    StandingDto::QueueRepeat { on } => StandingOrder::QueueRepeat { on: *on },
                    StandingDto::AutoPurchase { max_gold } => StandingOrder::AutoPurchase {
                        max_gold: *max_gold,
                    },
                },
            },
        })
    }
}

/// Governance actions as clients send them (`POST /api/gov`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum GovDto {
    /// Offices to stand for, by role name; empty withdraws.
    Stand {
        roles: Vec<String>,
    },
    Vote {
        role: String,
        candidate: MemberId,
    },
    Propose {
        role: String,
        orders: Vec<OrderDto>,
    },
    Support {
        proposal: u32,
    },
    Recall {
        role: String,
    },
}

impl GovDto {
    pub fn to_action(&self) -> Result<GovAction, String> {
        let role = |r: &str| parse_role(r).ok_or_else(|| format!("unknown office {r}"));
        Ok(match self {
            GovDto::Stand { roles } => GovAction::Stand {
                roles: roles
                    .iter()
                    .map(|r| role(r).map(|x| x.bit()))
                    .sum::<Result<u8, _>>()?,
            },
            GovDto::Vote { role: r, candidate } => GovAction::Vote {
                role: role(r)?,
                candidate: *candidate,
            },
            GovDto::Propose { role: r, orders } => GovAction::Propose {
                role: role(r)?,
                orders: orders
                    .iter()
                    .map(OrderDto::to_order)
                    .collect::<Result<_, _>>()?,
            },
            GovDto::Support { proposal } => GovAction::Support {
                proposal: *proposal,
            },
            GovDto::Recall { role: r } => GovAction::Recall { role: role(r)? },
        })
    }

    pub fn from_action(a: &GovAction) -> GovDto {
        match a {
            GovAction::Stand { roles } => GovDto::Stand {
                roles: Role::ALL
                    .iter()
                    .filter(|r| roles & r.bit() != 0)
                    .map(name)
                    .collect(),
            },
            GovAction::Vote { role, candidate } => GovDto::Vote {
                role: name(role),
                candidate: *candidate,
            },
            GovAction::Propose { role, orders } => GovDto::Propose {
                role: name(role),
                orders: orders.iter().map(OrderDto::from_order).collect(),
            },
            GovAction::Support { proposal } => GovDto::Support {
                proposal: *proposal,
            },
            GovAction::Recall { role } => GovDto::Recall { role: name(role) },
        }
    }
}
