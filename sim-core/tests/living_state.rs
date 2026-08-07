use sim_core::*;

fn apply(world: &mut World, command: Command) -> Vec<TimedEvent> {
    let outcome = world.apply(command);
    assert_eq!(outcome.error, None);
    outcome.events
}

fn spawn(world: &mut World, cell: u32, food: u32, water: u32) -> EntityId {
    match apply(
        world,
        Command::SpawnSoldier {
            spec: SoldierSpec {
                position: Position {
                    cell,
                    ..Position::default()
                },
                inventory: Inventory {
                    food,
                    water,
                    medical: 0,
                },
                ..SoldierSpec::default()
            },
        },
    )[0]
    .event
    {
        Event::SoldierSpawned { id, .. } => id,
        _ => unreachable!(),
    }
}

fn automatic(events: &[TimedEvent]) -> Vec<TimedEvent> {
    events
        .iter()
        .copied()
        .filter(|x| {
            matches!(
                x.event,
                Event::RationConsumed { .. }
                    | Event::LivingDeteriorated { .. }
                    | Event::ActivityChanged { forced: true, .. }
                    | Event::SoldierDied { .. }
            )
        })
        .collect()
}

#[test]
fn cluster_02_hot_and_cold_match_independent_boundaries() {
    let mut cold = World::new(7);
    let mut hot = World::new(7);
    let cold_id = spawn(&mut cold, 4, 3, 3);
    let hot_id = spawn(&mut hot, 4, 3, 3);
    assert_eq!(cold_id, hot_id);
    apply(
        &mut cold,
        Command::SetActivity {
            id: cold_id,
            activity: Activity::March,
        },
    );
    apply(
        &mut hot,
        Command::SetActivity {
            id: hot_id,
            activity: Activity::March,
        },
    );
    apply(&mut hot, Command::SetRegionHot { cell: 4, hot: true });
    let cold_events = apply(&mut cold, Command::AdvanceTo { target: 140 });
    let hot_events = apply(&mut hot, Command::AdvanceTo { target: 140 });
    assert_eq!(cold.soldier(cold_id), hot.soldier(hot_id));
    assert_eq!(automatic(&cold_events), automatic(&hot_events));
    assert!(cold.living_work_counters().0 < 20);
    assert_eq!(hot.living_work_counters().1, 140);
}

#[test]
fn cluster_06_activities_differ_and_invalid_activity_is_atomic() {
    let mut world = World::new(0);
    let rest = spawn(&mut world, 1, 1, 1);
    let idle = spawn(&mut world, 2, 1, 1);
    let march = spawn(&mut world, 3, 1, 1);
    apply(
        &mut world,
        Command::SetActivity {
            id: rest,
            activity: Activity::Rest,
        },
    );
    apply(
        &mut world,
        Command::SetActivity {
            id: march,
            activity: Activity::March,
        },
    );
    apply(&mut world, Command::AdvanceTo { target: 10 });
    assert_eq!(world.soldier(rest).unwrap().needs.fatigue, 0);
    assert_eq!(world.soldier(idle).unwrap().needs.fatigue, 10);
    assert_eq!(world.soldier(march).unwrap().needs.fatigue, 30);
    let digest = world.state_digest();
    assert_eq!(
        world
            .apply(Command::SetActivity {
                id: EntityId::from_raw(u64::MAX),
                activity: Activity::March
            })
            .error,
        Some(SimError::InvalidEntity)
    );
    assert_eq!(world.state_digest(), digest);
}

#[test]
fn cluster_04_consumption_removal_and_ledger_are_conserved_once() {
    let mut world = World::new(0);
    let fed = spawn(&mut world, 1, 1, 1);
    let doomed = spawn(&mut world, 2, 0, 0);
    let events = apply(&mut world, Command::AdvanceTo { target: 2_000 });
    assert!(events
        .iter()
        .any(|x| matches!(x.event, Event::RationConsumed { id, food: 1, .. } if id == fed)));
    assert_eq!(
        events
            .iter()
            .filter(|x| matches!(x.event, Event::SoldierDied { id, .. } if id == doomed))
            .count(),
        1
    );
    assert!(matches!(
        world.soldier(doomed).unwrap().living.life,
        LifeState::Dead { .. }
    ));
    let totals = world.resource_totals();
    assert_eq!(
        totals.sourced_food,
        totals.carried_food + totals.consumed_food + totals.lost_food
    );
    assert_eq!(
        totals.sourced_water,
        totals.carried_water + totals.consumed_water + totals.lost_water
    );
    apply(&mut world, Command::DespawnSoldier { id: fed });
    let totals = world.resource_totals();
    assert_eq!(
        totals.sourced_food,
        totals.carried_food + totals.consumed_food + totals.lost_food
    );
}

#[test]
fn cluster_03_repeated_fidelity_churn_preserves_state() {
    let mut world = World::new(91);
    let id = spawn(&mut world, 8, 4, 5);
    apply(&mut world, Command::AdvanceTo { target: 49 });
    apply(&mut world, Command::SetRegionHot { cell: 8, hot: true });
    apply(&mut world, Command::AdvanceTo { target: 73 });
    apply(
        &mut world,
        Command::SetRegionHot {
            cell: 8,
            hot: false,
        },
    );
    let mut restored = World::from_snapshot(&world.snapshot()).unwrap();
    let a = apply(&mut world, Command::AdvanceTo { target: 210 });
    let b = apply(&mut restored, Command::AdvanceTo { target: 210 });
    assert_eq!(a, b);
    assert_eq!(world.soldier(id), restored.soldier(id));
    assert_eq!(world.snapshot(), restored.snapshot());
    let before = world.snapshot();
    let _ = world.soldier(id);
    assert_eq!(world.snapshot(), before);
}

#[test]
fn cluster_10_queries_and_snapshot_resume_are_byte_deterministic() {
    let mut uninterrupted = World::new(91);
    let id = spawn(&mut uninterrupted, 8, 4, 5);
    apply(&mut uninterrupted, Command::AdvanceTo { target: 73 });
    let before_query = uninterrupted.snapshot();
    let _ = uninterrupted.soldier(id);
    assert_eq!(uninterrupted.snapshot(), before_query);
    let mut restored = World::from_snapshot(&before_query).unwrap();
    let a = apply(&mut uninterrupted, Command::AdvanceTo { target: 210 });
    let b = apply(&mut restored, Command::AdvanceTo { target: 210 });
    assert_eq!(a, b);
    assert_eq!(uninterrupted.state_digest(), restored.state_digest());
    assert_eq!(uninterrupted.snapshot(), restored.snapshot());
}

#[test]
fn cluster_01_one_second_oracle_has_independent_expected_values() {
    let cases = [
        (Activity::Rest, 0, 1, 1, 0),
        (Activity::Idle, 1, 1, 2, 1),
        (Activity::March, 3, 2, 3, 2),
    ];
    for (activity, fatigue, hunger, thirst, sleep_debt) in cases {
        let mut world = World::new(0);
        let id = spawn(&mut world, 1, 0, 0);
        apply(&mut world, Command::SetActivity { id, activity });
        apply(&mut world, Command::AdvanceTo { target: 1 });
        assert_eq!(
            world.soldier(id).unwrap().needs,
            Needs {
                fatigue,
                hunger,
                thirst,
                sleep_debt,
            }
        );
    }
}

#[test]
fn cluster_05_ration_and_deterioration_boundaries_are_exact() {
    let mut world = World::new(0);
    let id = spawn(&mut world, 1, 2, 2);
    assert!(automatic(&apply(&mut world, Command::AdvanceTo { target: 49 })).is_empty());
    let at_50 = automatic(&apply(&mut world, Command::AdvanceTo { target: 50 }));
    assert_eq!(at_50.len(), 1);
    assert!(
        matches!(at_50[0].event, Event::RationConsumed { id: got, food: 0, water: 1, .. } if got == id)
    );
    let at_100 = automatic(&apply(&mut world, Command::AdvanceTo { target: 100 }));
    assert_eq!(at_100.len(), 1);
    assert!(
        matches!(at_100[0].event, Event::RationConsumed { id: got, food: 1, water: 1, .. } if got == id)
    );
    assert_eq!(
        world.soldier(id).unwrap().inventory,
        Inventory {
            food: 1,
            water: 0,
            medical: 0
        }
    );
}

#[test]
fn cluster_09_hot_snapshot_and_canonical_due_are_strict() {
    let mut world = World::new(0);
    apply(&mut world, Command::SetRegionHot { cell: 9, hot: true });
    let id = spawn(&mut world, 9, 1, 1);
    apply(
        &mut world,
        Command::SetActivity {
            id,
            activity: Activity::March,
        },
    );
    let snapshot = world.snapshot();
    let mut restored = World::from_snapshot(&snapshot).unwrap();
    apply(
        &mut world,
        Command::SetRegionHot {
            cell: 9,
            hot: false,
        },
    );
    apply(
        &mut restored,
        Command::SetRegionHot {
            cell: 9,
            hot: false,
        },
    );
    assert_eq!(world.snapshot(), restored.snapshot());
    assert_eq!(
        apply(&mut world, Command::AdvanceTo { target: 100 }),
        apply(&mut restored, Command::AdvanceTo { target: 100 })
    );
}

#[test]
fn cluster_07_stale_generation_cannot_act_on_reused_soldier() {
    let mut world = World::new(0);
    let stale = spawn(&mut world, 1, 0, 0);
    apply(&mut world, Command::DespawnSoldier { id: stale });
    let current = spawn(&mut world, 1, 0, 0);
    assert_ne!(stale, current);
    let before = world.snapshot();
    assert_eq!(
        world
            .apply(Command::SetActivity {
                id: stale,
                activity: Activity::March
            })
            .error,
        Some(SimError::InvalidEntity)
    );
    assert_eq!(world.snapshot(), before);
    assert_eq!(
        world.soldier(current).unwrap().living.activity,
        Activity::Idle
    );
}

#[test]
fn cluster_08_automatic_work_precedes_same_time_scheduled_commands() {
    let mut world = World::new(0);
    let id = spawn(&mut world, 1, 1, 1);
    apply(
        &mut world,
        Command::Schedule {
            at: 50,
            command: ScheduledCommand::SetRegionHot { cell: 1, hot: true },
        },
    );
    let events = apply(&mut world, Command::AdvanceTo { target: 50 });
    let ration = events
        .iter()
        .position(|event| matches!(event.event, Event::RationConsumed { id: got, .. } if got == id))
        .unwrap();
    let command = events
        .iter()
        .position(|event| {
            matches!(
                event.event,
                Event::RegionFidelityChanged {
                    cell: 1,
                    hot: true,
                    ..
                }
            )
        })
        .unwrap();
    assert!(ration < command);
}

#[test]
fn cluster_11_huge_sparse_advance_has_exact_boundary_work() {
    let mut world = World::new(0);
    let id = spawn(&mut world, 1, 0, 0);
    let events = apply(
        &mut world,
        Command::AdvanceTo {
            target: 1_000_000_000,
        },
    );
    assert!(matches!(
        world.soldier(id).unwrap().living.life,
        LifeState::Dead {
            at: 499,
            cause: DeathCause::Dehydration
        }
    ));
    assert_eq!(world.living_work_counters(), (100, 0));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.event, Event::SoldierDied { id: got, .. } if got == id))
            .count(),
        1
    );
}

#[test]
fn cluster_12_hot_processing_touches_exact_indexed_membership() {
    let mut world = World::new(0);
    let hot_a = spawn(&mut world, 7, 2, 2);
    let removed = spawn(&mut world, 7, 2, 2);
    let cold = spawn(&mut world, 8, 2, 2);
    apply(&mut world, Command::DespawnSoldier { id: removed });
    apply(&mut world, Command::SetRegionHot { cell: 7, hot: true });
    apply(&mut world, Command::AdvanceTo { target: 3 });
    assert_eq!(world.living_work_counters(), (0, 3));
    assert_eq!(world.soldier(hot_a).unwrap().living.materialized_at, 3);
    assert_eq!(world.soldier(cold).unwrap().needs.hunger, 3);
    assert!(world.soldier(removed).is_none());
    assert!(World::from_snapshot(&world.snapshot()).is_ok());
}
