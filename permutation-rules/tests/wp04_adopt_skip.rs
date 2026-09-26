//! WP04 r2: an adopted proposal that would take a batch over the caps is
//! skipped (not adopted); the batch itself still runs.
mod common;
use common::nations::*;
use permutation_rules::gov::{self, GovAction, Role};
use permutation_rules::orders::{accept_batch, validate_batch, Order};

#[test]
fn over_cap_adoptions_are_skipped_not_fatal() {
    let (rules, mut s) = world(&[2, 0, 0, 0, 0, 0]);
    let mut e = Vec::new();
    elect(&s, &mut e, 0, &[Role::General]);
    gov::open_government(&mut s, &rules, &e).unwrap();
    let cw = |n: usize| {
        (0..n)
            .map(|k| Order::ConsentWar {
                civ: 1 + (k % 5) as u16,
            })
            .collect::<Vec<_>>()
    };
    step(
        &mut s,
        &rules,
        vec![],
        vec![
            act(
                1,
                GovAction::Propose {
                    role: Role::General,
                    orders: cw(4),
                },
            ),
            act(
                1,
                GovAction::Propose {
                    role: Role::General,
                    orders: cw(3),
                },
            ),
        ],
    );
    let ids: Vec<u32> = s.nations[0].proposals.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 2, "both proposals open");
    let mut b = batch(&s, 0, Role::General, 0, cw(5));
    b.adopt = ids.clone();
    // 5 own free + 4 would be 9 > 8: the first is skipped; 5 + 3 = 8 fits.
    let (_, orders, adopted) = accept_batch(&s, &rules, &b).unwrap();
    assert_eq!(adopted, vec![ids[1]]);
    assert_eq!(orders.len(), 8);
    assert!(validate_batch(&s, &rules, &b).is_ok());
    step(&mut s, &rules, vec![b], vec![]);
    let left: Vec<u32> = s.nations[0].proposals.iter().map(|p| p.id).collect();
    assert_eq!(
        left,
        vec![ids[0]],
        "the skipped proposal stays open; the other was adopted"
    );
    // Nine own free orders are refused whole.
    let b9 = batch(&s, 0, Role::General, 0, cw(9));
    assert!(validate_batch(&s, &rules, &b9).is_err());
}
