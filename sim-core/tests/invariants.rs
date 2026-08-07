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
        w.apply_world(WorldCommand::Transfer {
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
        WorldCommand::Transfer {
            from: 1,
            to: 2,
            ammunition: 2,
            supplies: 3,
        },
    )
    .unwrap();
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
fn transfer_adversarial_cases_are_atomic_and_conserved() {
    let mut w = World::new(3);
    w.set_stockpile(
        1,
        Stock {
            ammunition: 10_000,
            supplies: 20_000,
        },
    );
    w.set_stockpile(
        2,
        Stock {
            ammunition: 5,
            supplies: 7,
        },
    );
    w.set_stockpile(3, Stock::default());
    let before = (w.stockpile(1), w.stockpile(2), w.stockpile(3));
    assert_eq!(
        w.apply_world(WorldCommand::Transfer {
            from: 1,
            to: 1,
            ammunition: 1,
            supplies: 1
        }),
        Err(SimError::InvalidTransfer)
    );
    assert_eq!(
        w.apply_world(WorldCommand::Transfer {
            from: 9,
            to: 2,
            ammunition: 1,
            supplies: 1
        }),
        Err(SimError::UnknownStockpile)
    );
    assert_eq!(
        w.apply_world(WorldCommand::Transfer {
            from: 1,
            to: 9,
            ammunition: 1,
            supplies: 1
        }),
        Err(SimError::UnknownStockpile)
    );
    assert_eq!(
        w.apply_world(WorldCommand::Transfer {
            from: 2,
            to: 3,
            ammunition: 99,
            supplies: 0
        }),
        Err(SimError::InsufficientStock)
    );
    assert_eq!(before, (w.stockpile(1), w.stockpile(2), w.stockpile(3)));
    w.set_stockpile(
        3,
        Stock {
            ammunition: u64::MAX,
            supplies: 0,
        },
    );
    let overflow_before = (w.stockpile(1), w.stockpile(3));
    assert_eq!(
        w.apply_world(WorldCommand::Transfer {
            from: 1,
            to: 3,
            ammunition: 1,
            supplies: 0
        }),
        Err(SimError::ArithmeticOverflow)
    );
    assert_eq!(overflow_before, (w.stockpile(1), w.stockpile(3)));
    w.set_stockpile(3, Stock::default());
    for i in 0..10_000 {
        let (from, to) = if i % 3 == 0 {
            (1, 2)
        } else if i % 3 == 1 {
            (2, 3)
        } else {
            (3, 1)
        };
        w.apply_world(WorldCommand::Transfer {
            from,
            to,
            ammunition: 1,
            supplies: 1,
        })
        .unwrap();
    }
    assert_eq!(
        (1..=3)
            .map(|i| w.stockpile(i).unwrap().ammunition)
            .sum::<u64>(),
        10_005
    );
    assert_eq!(
        (1..=3)
            .map(|i| w.stockpile(i).unwrap().supplies)
            .sum::<u64>(),
        20_007
    );
}

#[test]
fn scheduler_is_monotonic_bounded_and_preserves_failed_work() {
    let mut w = World::new(0);
    w.advance_to(10).unwrap();
    assert_eq!(
        w.schedule(9, WorldCommand::SetRegionHot { cell: 1, hot: true }),
        Err(SimError::TimeReversal)
    );
    w.set_stockpile(1, Stock::default());
    w.set_stockpile(2, Stock::default());
    w.schedule(
        20,
        WorldCommand::Transfer {
            from: 1,
            to: 2,
            ammunition: 1,
            supplies: 0,
        },
    )
    .unwrap();
    w.schedule(20, WorldCommand::SetRegionHot { cell: 8, hot: true })
        .unwrap();
    assert_eq!(w.advance_to(30), Err(SimError::InsufficientStock));
    assert_eq!(w.clock(), 20);
    let failed_digest = w.state_digest();
    assert_eq!(w.advance_to(30), Err(SimError::InsufficientStock));
    assert_eq!(failed_digest, w.state_digest());
    let mut max = World::new(0);
    max.advance_to(u64::MAX).unwrap();
    assert_eq!(max.clock(), u64::MAX);
}

#[test]
fn allocator_and_relationships_survive_snapshot_exactly() {
    let mut a = World::new(4);
    a.create_squad(1);
    a.create_squad(2);
    let rifle = a.spawn(soldier(0)).unwrap();
    assert_eq!(
        a.assign_officer(1, rifle),
        Err(SimError::InvalidOfficerRole)
    );
    let officer = a
        .spawn(SoldierSpec {
            role: Role::Officer,
            ..soldier(0)
        })
        .unwrap();
    a.assign_officer(1, officer).unwrap();
    a.assign_officer(2, officer).unwrap();
    assert!(!a.squad(1).unwrap().members.contains(&officer));
    assert_eq!(a.squad(1).unwrap().officer, None);
    assert!(a.squad(2).unwrap().members.contains(&officer));
    let ids: Vec<_> = (0..8).map(|_| a.spawn(soldier(0)).unwrap()).collect();
    a.despawn(ids[1]);
    a.despawn(ids[5]);
    a.despawn(rifle);
    let mut b = World::from_snapshot(&a.snapshot()).unwrap();
    for _ in 0..10 {
        assert_eq!(a.spawn(soldier(0)).unwrap(), b.spawn(soldier(0)).unwrap());
    }
    assert_eq!(a.state_digest(), b.state_digest());
    assert!(a.despawn(officer));
    assert_eq!(a.squad(2).unwrap().officer, None);
}

#[test]
fn needs_saturate_after_long_duration() {
    let mut w = World::new(0);
    let id = w.spawn(soldier(0)).unwrap();
    w.advance_to(u64::MAX).unwrap();
    assert_eq!(
        w.soldier(id).unwrap().needs,
        Needs {
            fatigue: u32::MAX,
            hunger: u32::MAX,
            thirst: u32::MAX,
            sleep_debt: u32::MAX
        }
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
    w.apply_world(WorldCommand::Transfer {
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
        w.apply_world(WorldCommand::Transfer {
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
    w.apply_world(WorldCommand::SetRegionHot { cell: 5, hot: true })
        .unwrap();
    w.advance_to(30).unwrap();
    w.apply_world(WorldCommand::SetRegionHot {
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
