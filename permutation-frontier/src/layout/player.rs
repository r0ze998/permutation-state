//! Player accounts (§5.3): Citizen, Holding (with its transit records and
//! explore record). Offsets: `frontier_abi::layout::player`.

pub use frontier_abi::layout::player::{
    accrual, citizen, explore, holding, holding_ref, queue_item, ticket_site, transit,
};

/// Offset of transit record `i` in a Holding.
pub const fn transit_at(i: usize) -> usize {
    holding::transit(i)
}
