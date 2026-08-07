use sim_core::*;

fn ok(w: &mut World, c: Command) -> Vec<TimedEvent> {
    let o = w.apply(c);
    assert_eq!(o.error, None);
    o.events
}
fn spec(ammunition: u32) -> SoldierSpec {
    SoldierSpec {
        ammunition,
        inventory: Inventory {
            food: 2,
            water: 3,
            medical: 1,
        },
        ..SoldierSpec::default()
    }
}

#[test]
fn lifecycle_is_observable_and_conserved_by_explicit_sources_and_losses() {
    let mut w = World::new(1);
    let before = w.resource_totals();
    let e = ok(&mut w, Command::SpawnSoldier { spec: spec(30) });
    let id = match e[0].event {
        Event::SoldierSpawned { id, loadout } => {
            assert_eq!(
                loadout,
                Loadout {
                    ammunition: 30,
                    food: 2,
                    water: 3,
                    medical: 1
                }
            );
            id
        }
        _ => panic!(),
    };
    assert_eq!(
        w.resource_totals(),
        ResourceTotals {
            ammunition: 30,
            stockpile_supplies: 0,
            carried_food: 2,
            carried_water: 3,
            carried_medical: 1
        }
    );
    let e = ok(&mut w, Command::DespawnSoldier { id });
    assert!(matches!(e[0].event,Event::SoldierRemoved{id:x,..} if x==id));
    assert_eq!(w.resource_totals(), before);
    let digest = w.state_digest();
    assert_eq!(
        w.apply(Command::DespawnSoldier { id }).error,
        Some(SimError::InvalidEntity)
    );
    assert_eq!(w.state_digest(), digest)
}

#[test]
fn scheduled_events_span_timestamps() {
    let mut w = World::new(0);
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 1,
            initial: Stock {
                ammunition: 4,
                supplies: 0,
            },
        },
    );
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 2,
            initial: Stock::default(),
        },
    );
    ok(
        &mut w,
        Command::Schedule {
            at: 2,
            command: ScheduledCommand::Transfer {
                from: 1,
                to: 2,
                ammunition: 1,
                supplies: 0,
            },
        },
    );
    ok(
        &mut w,
        Command::Schedule {
            at: 5,
            command: ScheduledCommand::SetRegionHot { cell: 9, hot: true },
        },
    );
    let o = w.apply(Command::AdvanceTo { target: 8 });
    assert_eq!(o.error, None);
    let command_events: Vec<_> = o
        .events
        .iter()
        .filter(|event| !matches!(event.event, Event::TimeAdvanced { .. }))
        .collect();
    assert_eq!(command_events.len(), 2);
    assert_eq!((command_events[0].at, command_events[1].at), (2, 5));
    assert_eq!(o.clock, 8)
}

#[test]
fn dense_failure_returns_prefix_and_retains_exact_suffix() {
    let mut w = World::new(0);
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 1,
            initial: Stock {
                ammunition: 3,
                supplies: 0,
            },
        },
    );
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 2,
            initial: Stock::default(),
        },
    );
    for cell in 0..1000 {
        ok(
            &mut w,
            Command::Schedule {
                at: 7,
                command: ScheduledCommand::SetRegionHot { cell, hot: true },
            },
        );
    }
    let fail_id = match ok(
        &mut w,
        Command::Schedule {
            at: 7,
            command: ScheduledCommand::Transfer {
                from: 1,
                to: 2,
                ammunition: 4,
                supplies: 0,
            },
        },
    )[0]
    .event
    {
        Event::Scheduled { id, .. } => id,
        _ => panic!(),
    };
    for cell in 1000..2000 {
        ok(
            &mut w,
            Command::Schedule {
                at: 7,
                command: ScheduledCommand::SetRegionHot { cell, hot: true },
            },
        );
    }
    let o = w.apply(Command::AdvanceTo { target: 10 });
    assert_eq!(
        o.events
            .iter()
            .filter(|event| matches!(event.event, Event::RegionFidelityChanged { .. }))
            .count(),
        1000
    );
    assert_eq!(o.error, Some(SimError::InsufficientStock));
    assert_eq!(o.clock, 7);
    assert_eq!(w.hot_cell_count(), 1000);
    let snap = w.snapshot();
    let mut restored = World::from_snapshot(&snap).unwrap();
    assert!(matches!(
        ok(&mut w, Command::CancelScheduled { id: fail_id })[0].event,
        Event::ScheduleCancelled { .. }
    ));
    ok(&mut restored, Command::CancelScheduled { id: fail_id });
    let a = w.apply(Command::AdvanceTo { target: 10 });
    let b = restored.apply(Command::AdvanceTo { target: 10 });
    assert_eq!(a, b);
    assert_eq!(
        a.events
            .iter()
            .filter(|event| matches!(event.event, Event::RegionFidelityChanged { .. }))
            .count(),
        1000
    );
    assert_eq!(w.state_digest(), restored.state_digest())
}

#[test]
fn scheduler_rejects_intrinsic_invalidity_and_can_repair_state_failure() {
    let mut w = World::new(0);
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 1,
            initial: Stock::default(),
        },
    );
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 2,
            initial: Stock::default(),
        },
    );
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 3,
            initial: Stock {
                ammunition: 5,
                supplies: 0,
            },
        },
    );
    assert_eq!(
        w.apply(Command::Schedule {
            at: 1,
            command: ScheduledCommand::Transfer {
                from: 1,
                to: 1,
                ammunition: 1,
                supplies: 0
            }
        })
        .error,
        Some(SimError::InvalidTransfer)
    );
    assert_eq!(
        w.apply(Command::Schedule {
            at: 1,
            command: ScheduledCommand::CreateStockpile {
                id: 1,
                initial: Stock::default()
            }
        })
        .error,
        Some(SimError::StockpileAlreadyExists)
    );
    ok(
        &mut w,
        Command::Schedule {
            at: 2,
            command: ScheduledCommand::Transfer {
                from: 1,
                to: 2,
                ammunition: 2,
                supplies: 0,
            },
        },
    );
    assert_eq!(
        w.apply(Command::AdvanceTo { target: 3 }).error,
        Some(SimError::InsufficientStock)
    );
    ok(
        &mut w,
        Command::Transfer {
            from: 3,
            to: 1,
            ammunition: 2,
            supplies: 0,
        },
    );
    assert_eq!(w.apply(Command::AdvanceTo { target: 3 }).error, None);
    assert_eq!(w.stockpile(2).unwrap().ammunition, 2)
}

#[test]
fn hot_cells_step_only_after_activation_and_survive_snapshot() {
    let mut w = World::new(0);
    ok(&mut w, Command::AdvanceTo { target: 5 });
    ok(&mut w, Command::SetRegionHot { cell: 1, hot: true });
    ok(
        &mut w,
        Command::Schedule {
            at: 8,
            command: ScheduledCommand::SetRegionHot { cell: 2, hot: true },
        },
    );
    ok(
        &mut w,
        Command::Schedule {
            at: 10,
            command: ScheduledCommand::SetRegionHot {
                cell: 1,
                hot: false,
            },
        },
    );
    ok(&mut w, Command::AdvanceTo { target: 12 });
    assert_eq!(w.hot_cell(1), None);
    assert_eq!(
        w.hot_cell(2),
        Some(HotCellState {
            activated_at: 8,
            last_stepped_at: 12,
            fixed_steps: 4
        })
    );
    let mut r = World::from_snapshot(&w.snapshot()).unwrap();
    ok(&mut w, Command::AdvanceTo { target: 20 });
    ok(&mut r, Command::AdvanceTo { target: 20 });
    assert_eq!(w.state_digest(), r.state_digest());
    let mut max = World::new(0);
    ok(&mut max, Command::SetRegionHot { cell: 0, hot: true });
    assert_eq!(
        max.apply(Command::AdvanceTo { target: u64::MAX }).error,
        None
    );
    assert_eq!(max.hot_cell(0).unwrap().fixed_steps, u64::MAX)
}

#[test]
fn relationships_and_ids_replay() {
    let mut a = World::new(2);
    ok(&mut a, Command::CreateSquad { id: 1 });
    ok(&mut a, Command::CreateSquad { id: 2 });
    let officer = match ok(
        &mut a,
        Command::SpawnSoldier {
            spec: SoldierSpec {
                role: Role::Officer,
                ..SoldierSpec::default()
            },
        },
    )[0]
    .event
    {
        Event::SoldierSpawned { id, .. } => id,
        _ => panic!(),
    };
    ok(&mut a, Command::AssignOfficer { squad: 1, officer });
    ok(&mut a, Command::AssignOfficer { squad: 2, officer });
    assert!(a.squad(1).unwrap().members.is_empty());
    let mut b = World::from_snapshot(&a.snapshot()).unwrap();
    for _ in 0..20 {
        let ia = match ok(
            &mut a,
            Command::SpawnSoldier {
                spec: SoldierSpec::default(),
            },
        )[0]
        .event
        {
            Event::SoldierSpawned { id, .. } => id,
            _ => panic!(),
        };
        let ib = match ok(
            &mut b,
            Command::SpawnSoldier {
                spec: SoldierSpec::default(),
            },
        )[0]
        .event
        {
            Event::SoldierSpawned { id, .. } => id,
            _ => panic!(),
        };
        assert_eq!(ia, ib)
    }
    assert_eq!(a.state_digest(), b.state_digest())
}

#[test]
fn malformed_snapshot_is_atomic_to_caller() {
    let w = World::new(0);
    let mut b = w.snapshot();
    b.push(0);
    assert!(matches!(
        World::from_snapshot(&b),
        Err(SimError::Snapshot("trailing bytes"))
    ))
}

#[test]
fn same_seed_commands_produce_identical_events_and_digest() {
    let mut a = World::new(77);
    let mut b = World::new(77);
    let commands = [
        Command::NextRandom,
        Command::SpawnSoldier { spec: spec(9) },
        Command::AdvanceTo { target: 12 },
        Command::NextRandom,
    ];
    for command in commands {
        assert_eq!(a.apply(command), b.apply(command));
    }
    assert_eq!(a.state_digest(), b.state_digest());
}

#[test]
fn ledger_supplies_and_failed_transfers_are_atomic() {
    let mut w = World::new(1);
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 1,
            initial: Stock {
                ammunition: 20,
                supplies: 30,
            },
        },
    );
    ok(
        &mut w,
        Command::CreateStockpile {
            id: 2,
            initial: Stock {
                ammunition: u64::MAX - 5,
                supplies: u64::MAX - 5,
            },
        },
    );
    let totals = w.resource_totals();
    assert_eq!(totals.stockpile_supplies, u128::from(u64::MAX) + 25);
    for command in [
        Command::Transfer {
            from: 1,
            to: 1,
            ammunition: 1,
            supplies: 1,
        },
        Command::Transfer {
            from: 9,
            to: 2,
            ammunition: 1,
            supplies: 1,
        },
        Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 21,
            supplies: 1,
        },
        Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 6,
            supplies: 6,
        },
    ] {
        let digest = w.state_digest();
        assert!(w.apply(command).error.is_some());
        assert_eq!(w.resource_totals(), totals);
        assert_eq!(w.state_digest(), digest);
    }
    let event = ok(
        &mut w,
        Command::Transfer {
            from: 1,
            to: 2,
            ammunition: 5,
            supplies: 5,
        },
    );
    assert!(matches!(
        event[0].event,
        Event::TransferCompleted {
            stock: Stock {
                ammunition: 5,
                supplies: 5
            },
            ..
        }
    ));
    assert_eq!(w.resource_totals(), totals);
}

#[test]
fn elapsed_needs_saturate_and_time_never_reverses() {
    let mut w = World::new(0);
    let id = match ok(
        &mut w,
        Command::SpawnSoldier {
            spec: SoldierSpec::default(),
        },
    )[0]
    .event
    {
        Event::SoldierSpawned { id, .. } => id,
        _ => unreachable!(),
    };
    ok(&mut w, Command::AdvanceTo { target: u64::MAX });
    assert_eq!(
        w.soldier(id).unwrap().needs,
        Needs {
            fatigue: u32::MAX,
            hunger: u32::MAX,
            thirst: u32::MAX,
            sleep_debt: u32::MAX
        }
    );
    let digest = w.state_digest();
    assert_eq!(
        w.apply(Command::AdvanceTo {
            target: u64::MAX - 1
        })
        .error,
        Some(SimError::TimeReversal)
    );
    assert_eq!(w.clock(), u64::MAX);
    assert_eq!(w.state_digest(), digest);
}

#[test]
fn indexed_cancellation_preserves_large_same_time_order() {
    let mut w = World::new(0);
    let mut ids = Vec::new();
    for cell in 0..100_000u32 {
        let e = ok(
            &mut w,
            Command::Schedule {
                at: 1,
                command: ScheduledCommand::SetRegionHot { cell, hot: true },
            },
        );
        if let Event::Scheduled { id, .. } = e[0].event {
            ids.push(id)
        }
    }
    ok(&mut w, Command::CancelScheduled { id: ids[99_998] });
    let out = w.apply(Command::AdvanceTo { target: 1 });
    assert_eq!(out.error, None);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e.event, Event::RegionFidelityChanged { .. }))
            .count(),
        99_999
    );
    assert!(
        w.hot_cell(99_997).is_some()
            && w.hot_cell(99_998).is_none()
            && w.hot_cell(99_999).is_some()
    );
}

#[test]
fn relationships_cleanup_at_scale_and_maximum_squad_round_trip() {
    let mut w = World::new(0);
    for id in 0..10_000 {
        ok(&mut w, Command::CreateSquad { id });
    }
    ok(&mut w, Command::CreateSquad { id: u32::MAX });
    let officer = match ok(
        &mut w,
        Command::SpawnSoldier {
            spec: SoldierSpec {
                role: Role::Officer,
                ..SoldierSpec::default()
            },
        },
    )[0]
    .event
    {
        Event::SoldierSpawned { id, .. } => id,
        _ => unreachable!(),
    };
    ok(
        &mut w,
        Command::AssignOfficer {
            squad: 9_999,
            officer,
        },
    );
    ok(
        &mut w,
        Command::AssignOfficer {
            squad: u32::MAX,
            officer,
        },
    );
    assert!(w.squad(9_999).unwrap().members.is_empty());
    assert_eq!(
        World::from_snapshot(&w.snapshot()).unwrap().state_digest(),
        w.state_digest()
    );
    ok(&mut w, Command::DespawnSoldier { id: officer });
    assert!(w.squad(u32::MAX).unwrap().members.is_empty());
}

#[test]
fn deactivation_reports_exact_steps_across_cycles_and_partial_failure() {
    let mut w = World::new(0);
    ok(&mut w, Command::AdvanceTo { target: 3 });
    ok(&mut w, Command::SetRegionHot { cell: 4, hot: true });
    ok(
        &mut w,
        Command::Schedule {
            at: 8,
            command: ScheduledCommand::SetRegionHot {
                cell: 4,
                hot: false,
            },
        },
    );
    let out = w.apply(Command::AdvanceTo { target: 8 });
    assert!(out.events.iter().any(|e| matches!(
        e.event,
        Event::RegionFidelityChanged {
            cell: 4,
            hot: false,
            fixed_steps: 5
        }
    )));
    ok(&mut w, Command::SetRegionHot { cell: 4, hot: true });
    ok(&mut w, Command::AdvanceTo { target: 10 });
    let event = ok(
        &mut w,
        Command::SetRegionHot {
            cell: 4,
            hot: false,
        },
    );
    assert!(matches!(
        event[0].event,
        Event::RegionFidelityChanged { fixed_steps: 2, .. }
    ));
}
