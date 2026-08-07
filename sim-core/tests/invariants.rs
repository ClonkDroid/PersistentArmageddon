use sim_core::*;
use std::collections::BTreeSet;

fn soldier(cell: u32) -> SoldierSpec {
    SoldierSpec {
        position: Position {
            x_mm: 1,
            y_mm: 2,
            cell,
        },
        ammunition: 30,
        inventory: Inventory {
            food: 2,
            water: 3,
            medical: 1,
        },
        ..SoldierSpec::default()
    }
}

#[test]
fn ids_are_unique_across_reuse() {
    let mut w = World::new(1);
    let a = w.spawn(soldier(0)).unwrap();
    assert!(w.despawn(a));
    let b = w.spawn(soldier(0)).unwrap();
    assert_ne!(a, b);
    assert!(w.soldier(a).is_none());
    assert!(w.soldier(b).is_some());
}
#[test]
fn replay_is_deterministic() {
    fn run() -> u64 {
        let mut w = World::new(44);
        w.set_stockpile(
            1,
            Stock {
                ammunition: 100,
                supplies: 50,
            },
        );
        w.set_stockpile(2, Stock::default());
        for c in 0..100 {
            w.spawn(soldier(c % 3)).unwrap();
        }
        w.apply(Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 7,
            supplies: 9,
        })
        .unwrap();
        w.advance_to(90).unwrap();
        w.state_digest()
    }
    assert_eq!(run(), run());
}
#[test]
fn snapshot_resume_matches() {
    let mut a = World::new(9);
    a.spawn(soldier(4)).unwrap();
    a.set_stockpile(
        1,
        Stock {
            ammunition: 10,
            supplies: 10,
        },
    );
    a.set_stockpile(2, Stock::default());
    a.schedule(
        100,
        Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 2,
            supplies: 3,
        },
    );
    a.advance_to(60).unwrap();
    let mut b = World::from_snapshot(&a.snapshot()).unwrap();
    a.advance_to(121).unwrap();
    b.advance_to(121).unwrap();
    assert_eq!(a.state_digest(), b.state_digest());
    assert_eq!(
        b.stockpile(2),
        Some(Stock {
            ammunition: 2,
            supplies: 3
        })
    );
}
#[test]
fn transfers_conserve_and_fail_atomically() {
    let mut w = World::new(0);
    w.set_stockpile(
        1,
        Stock {
            ammunition: 10,
            supplies: 20,
        },
    );
    w.set_stockpile(
        2,
        Stock {
            ammunition: 3,
            supplies: 4,
        },
    );
    let total = Stock {
        ammunition: 13,
        supplies: 24,
    };
    w.apply(Command::Transfer {
        from: 1,
        to: 2,
        ammunition: 5,
        supplies: 6,
    })
    .unwrap();
    assert_eq!(
        w.stockpile(1).unwrap().ammunition + w.stockpile(2).unwrap().ammunition,
        total.ammunition
    );
    let before = (w.stockpile(1), w.stockpile(2));
    assert_eq!(
        w.apply(Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 99,
            supplies: 0
        }),
        Err(SimError::InsufficientStock)
    );
    assert_eq!(before, (w.stockpile(1), w.stockpile(2)));
}
#[test]
fn needs_are_lazily_materialized() {
    let mut w = World::new(0);
    let id = w.spawn(soldier(0)).unwrap();
    w.advance_to(120).unwrap();
    assert_eq!(
        w.soldier(id).unwrap().needs,
        Needs {
            fatigue: 120,
            hunger: 40,
            thirst: 60,
            sleep_debt: 30
        }
    );
}
#[test]
fn dead_officer_references_are_cleaned() {
    let mut w = World::new(0);
    w.create_squad(7);
    let id = w
        .spawn(SoldierSpec {
            squad: Some(7),
            role: Role::Officer,
            ..soldier(0)
        })
        .unwrap();
    w.assign_officer(7, id).unwrap();
    w.despawn(id);
    assert_eq!(w.squad(7).unwrap().officer, None);
    assert!(w.squad(7).unwrap().members.is_empty());
}
#[test]
fn fidelity_transitions_preserve_authority() {
    let mut w = World::new(0);
    let ids: BTreeSet<_> = (0..100).map(|_| w.spawn(soldier(5)).unwrap()).collect();
    w.apply(Command::SetRegionHot { cell: 5, hot: true })
        .unwrap();
    w.advance_to(30).unwrap();
    w.apply(Command::SetRegionHot {
        cell: 5,
        hot: false,
    })
    .unwrap();
    assert_eq!(w.soldier_count(), 100);
    assert_eq!(
        ids.iter().filter(|id| w.soldier(**id).is_some()).count(),
        100
    );
}
