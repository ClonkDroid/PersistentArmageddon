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
fn hot_and_cold_use_identical_living_transitions_at_boundaries() {
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
fn activities_differ_and_invalid_activity_is_atomic() {
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
fn consumption_death_and_removal_are_conserved_once() {
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
fn snapshot_resume_and_fidelity_churn_are_byte_deterministic() {
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
