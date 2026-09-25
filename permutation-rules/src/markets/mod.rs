//! Markets (§11): trade hubs, the gold AMM and the USDC market.
//!
//! Both markets clear **once per tick at one price**, so the order in which
//! orders arrived never matters (V4 §5.2):
//! - the AMM nets all orders per pool and executes only the net against the
//!   constant-product curve; every trader pays or receives the same price;
//! - the USDC market (V5 §7.5) runs a uniform-price call auction per good
//!   between nation treasuries, with a rising tariff on the buyer's
//!   cumulative spend, delivery after `delivery_ticks`, and no trades with
//!   oneself or an enemy.
//!
//! The clearing functions are pure (`clear_amm`, `call_auction`) so clients
//! and agents can forecast with exactly the engine's arithmetic.

mod amm;
mod usdc;

pub use amm::*;
pub use usdc::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::Side;
    use crate::params::Preset;
    use crate::params::Ruleset;
    use alloc::vec;

    fn rules() -> Ruleset {
        Ruleset::new(Preset::Blitz)
    }

    fn buy(qty: u32, limit: u32) -> AmmOrder {
        AmmOrder {
            side: Side::Buy,
            qty,
            limit_gold: limit,
            budget_milli: i64::MAX,
        }
    }
    fn sell(qty: u32, limit: u32) -> AmmOrder {
        AmmOrder {
            side: Side::Sell,
            qty,
            limit_gold: limit,
            budget_milli: 0,
        }
    }

    #[test]
    fn amm_single_buy_follows_the_curve_and_keeps_k() {
        let r = rules();
        let (x, y) = (200_000i64, 2_000_000i64); // 200 iron, 2000 gold
        let c = clear_amm(&r, x, y, &[buy(10, 1_000)]);
        // Exact curve: new_y = ceil(k / 190_000) = 2_105_264 → 105_264 in, price ceil(105_264/10).
        assert_eq!(c.price_milli, 10_527);
        assert_eq!(c.pool_goods_delta, -10_000);
        assert_eq!(c.pool_gold_delta, 105_270);
        let k0 = x as i128 * y as i128;
        let k1 = (x + c.pool_goods_delta) as i128 * (y + c.pool_gold_delta) as i128;
        assert!(k1 >= k0, "the curve invariant never decreases");
        let (gold, goods) = c.fills[0].unwrap();
        assert_eq!(goods, 10);
        assert_eq!(gold, -(105_270 + 105_270 * 300 / 10_000));
    }

    #[test]
    fn amm_batch_gives_everyone_the_same_price() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(5, 1_000), buy(5, 1_000)]);
        let (g1, _) = c.fills[0].unwrap();
        let (g2, _) = c.fills[1].unwrap();
        assert_eq!(g1, g2);
        let solo = clear_amm(&r, 200_000, 2_000_000, &[buy(10, 1_000)]);
        assert_eq!(c.price_milli, solo.price_milli, "two 5s clear like one 10");
    }

    #[test]
    fn amm_nets_opposite_orders_at_spot() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(7, 1_000), sell(7, 0)]);
        assert_eq!(c.price_milli, 10_000); // spot: 2000 gold / 200 iron
        assert_eq!((c.pool_goods_delta, c.pool_gold_delta), (0, 0));
    }

    #[test]
    fn amm_conserves_gold_exactly() {
        let r = rules();
        let orders = [buy(12, 1_000), sell(3, 0), buy(4, 1_000), sell(20, 0)];
        let c = clear_amm(&r, 200_000, 2_000_000, &orders);
        let traders: i64 = c.fills.iter().flatten().map(|(g, _)| *g).sum();
        // What traders lose (net) = what the pool gains + fees.
        assert_eq!(
            -traders,
            c.pool_gold_delta + c.hub_fee_milli + c.burned_milli
        );
        let goods: i64 = c.fills.iter().flatten().map(|(_, q)| *q * 1000).sum();
        assert_eq!(goods, -c.pool_goods_delta);
    }

    #[test]
    fn amm_drops_limit_violators_and_reprices() {
        let r = rules();
        // A buyer willing to pay only 50 gold for 10 iron (~108 needed) is dropped.
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(10, 50), buy(3, 1_000)]);
        assert!(c.fills[0].is_none());
        assert!(c.fills[1].is_some());
        assert_eq!(
            c.price_milli,
            clear_amm(&r, 200_000, 2_000_000, &[buy(3, 1_000)]).price_milli
        );
    }

    #[test]
    fn amm_rejects_draining_the_pool() {
        let r = rules();
        let c = clear_amm(&r, 200_000, 2_000_000, &[buy(250, u32::MAX)]);
        assert!(c.fills[0].is_none());
    }

    fn bid(qty: u32, price: u64, key: u64) -> Bid {
        Bid { qty, price, key }
    }

    #[test]
    fn call_auction_maximises_volume_at_one_price() {
        // Buyers: 10 @ 5, 10 @ 3. Sellers: 8 @ 2, 8 @ 4.
        let (p, b, s) = call_auction(
            &[bid(10, 5, 1), bid(10, 3, 2)],
            &[bid(8, 2, 3), bid(8, 4, 4)],
        )
        .unwrap();
        // At 3: demand 20, supply 8 → 8. At 4: demand 10, supply 16 → 10. At 5: 10/16 → 10.
        assert_eq!(
            p, 4,
            "max volume 10; tie between 4 and 5 broken by imbalance (6 vs 6) then lowest price"
        );
        assert_eq!(b, vec![10, 0]);
        assert_eq!(s, vec![8, 2]);
    }

    #[test]
    fn call_auction_breaks_ties_by_key() {
        let (_, b, _) = call_auction(&[bid(5, 5, 9), bid(5, 5, 1)], &[bid(5, 5, 3)]).unwrap();
        assert_eq!(b, vec![0, 5]);
    }

    #[test]
    fn call_auction_needs_crossing_orders() {
        assert!(call_auction(&[bid(5, 2, 1)], &[bid(5, 3, 2)]).is_none());
    }
}
