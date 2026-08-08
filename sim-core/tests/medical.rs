use sim_core::*;

fn spawn(world: &mut World, role: Role, medical: u32) -> EntityId {
    let out = world.apply(Command::SpawnSoldier {
        spec: SoldierSpec {
            role,
            inventory: Inventory {
                medical,
                ..Inventory::default()
            },
            ..SoldierSpec::default()
        },
    });
    match out.events[0].event {
        Event::SoldierSpawned { id, .. } => id,
        _ => unreachable!(),
    }
}

#[test]
fn distinct_wounds_bleed_and_snapshot_deterministically() {
    let mut w = World::new(7);
    let patient = spawn(&mut w, Role::Rifle, 0);
    for trauma in [10, 20] {
        assert!(w
            .apply(Command::InflictWound {
                patient,
                wound: WoundSpec {
                    trauma,
                    bleeding_per_second: 5,
                    shock: 10
                }
            })
            .error
            .is_none());
    }
    assert_eq!(w.wounds_of(patient).len(), 2);
    w.apply(Command::AdvanceTo { target: 3 });
    let c = w.casualty_state(patient).unwrap();
    assert_eq!(c.blood, BLOOD_MAX - 30);
    assert_eq!(c.shock, 23);
    let restored = World::from_snapshot(&w.snapshot()).unwrap();
    assert_eq!(restored.snapshot(), w.snapshot());
    assert_eq!(restored.state_digest(), w.state_digest());
}

#[test]
fn lowest_eligible_medic_consumes_once_and_completion_controls_wound() {
    let mut w = World::new(1);
    let medic0 = spawn(&mut w, Role::Medic, 2);
    let _medic1 = spawn(&mut w, Role::Medic, 2);
    let patient = spawn(&mut w, Role::Rifle, 0);
    w.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 1,
            bleeding_per_second: 2,
            shock: 1,
        },
    });
    let wound = w.wounds_of(patient)[0].id;
    let started = w.apply(Command::RequestTreatment {
        patient,
        wound: Some(wound),
        kind: TreatmentKind::Hemostatic,
    });
    let tid = match started.events[0].event {
        Event::TreatmentStarted { id, medic, .. } => {
            assert_eq!(medic, medic0);
            id
        }
        _ => panic!(),
    };
    assert_eq!(w.resource_totals().consumed_medical, 1);
    w.apply(Command::AdvanceTo {
        target: HEMOSTATIC_DURATION - 1,
    });
    assert!(!w.wound(wound).unwrap().controlled);
    w.apply(Command::AdvanceTo {
        target: HEMOSTATIC_DURATION,
    });
    assert!(w.wound(wound).unwrap().controlled);
    assert!(matches!(
        w.treatment(tid).unwrap().status,
        TreatmentStatus::Completed { .. }
    ));
    assert_eq!(w.resource_totals().consumed_medical, 1);
}

#[test]
fn interruption_releases_endpoints_without_refund() {
    let mut w = World::new(1);
    let medic = spawn(&mut w, Role::Medic, 3);
    let patient = spawn(&mut w, Role::Rifle, 0);
    w.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 1,
            bleeding_per_second: 1,
            shock: 1,
        },
    });
    let wound = w.wounds_of(patient)[0].id;
    let out = w.apply(Command::StartTreatment {
        medic,
        patient,
        wound: Some(wound),
        kind: TreatmentKind::Hemostatic,
    });
    let id = match out.events[0].event {
        Event::TreatmentStarted { id, .. } => id,
        _ => panic!(),
    };
    assert!(w.apply(Command::InterruptTreatment { id }).error.is_none());
    assert_eq!(w.resource_totals().consumed_medical, 1);
    assert!(w
        .apply(Command::StartTreatment {
            medic,
            patient,
            wound: Some(wound),
            kind: TreatmentKind::Hemostatic
        })
        .error
        .is_none());
}

#[test]
fn direct_advance_stamps_hemorrhage_at_the_causal_second() {
    let mut world = World::new(11);
    let patient = spawn(&mut world, Role::Rifle, 0);
    world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 100,
            shock: 0,
        },
    });
    let outcome = world.apply(Command::AdvanceTo { target: 100 });
    assert!(outcome.error.is_none());
    assert!(outcome.events.iter().any(|event| {
        event.at == 50
            && matches!(
                event.event,
                Event::SoldierDied {
                    id,
                    cause: DeathCause::Hemorrhage,
                    ..
                } if id == patient
            )
    }));
    assert_eq!(
        world.soldier(patient).unwrap().living.life,
        LifeState::Dead {
            at: 50,
            cause: DeathCause::Hemorrhage
        }
    );
    assert_eq!(
        World::from_snapshot(&world.snapshot()).unwrap().snapshot(),
        world.snapshot()
    );
}

#[test]
fn wounding_a_lazy_soldier_preserves_living_projection() {
    let mut world = World::new(12);
    let patient = spawn(&mut world, Role::Rifle, 0);
    world.apply(Command::AdvanceTo { target: 10 });
    world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 1,
            bleeding_per_second: 1,
            shock: 1,
        },
    });
    let living = world.soldier(patient).unwrap().living;
    assert_eq!((living.hunger, living.thirst, living.fatigue), (10, 20, 10));
    assert_eq!(living.materialized_at, 10);
}

#[test]
fn shock_treatment_rejects_an_unaffected_patient_without_consumption() {
    let mut world = World::new(13);
    let medic = spawn(&mut world, Role::Medic, SHOCK_TREATMENT_COST);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let before = world.snapshot();
    let outcome = world.apply(Command::StartTreatment {
        medic,
        patient,
        wound: None,
        kind: TreatmentKind::Shock,
    });
    assert_eq!(outcome.error, Some(SimError::InvalidTreatment));
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.resource_totals().consumed_medical, 0);
}

fn wound_event_id(outcome: &ApplyOutcome) -> WoundId {
    match outcome.events[0].event {
        Event::WoundInflicted { id, .. } => id,
        _ => panic!("expected wound event"),
    }
}

#[test]
fn bleeding_rate_seven_has_exact_hot_cold_remainder_and_death_fixture() {
    fn fixture(hot: bool) -> (CasualtyState, LifeState, Vec<TimedEvent>) {
        let mut world = World::new(71);
        let patient = match world
            .apply(Command::SpawnSoldier {
                spec: SoldierSpec {
                    inventory: Inventory {
                        food: 100,
                        water: 100,
                        ..Inventory::default()
                    },
                    ..SoldierSpec::default()
                },
            })
            .events[0]
            .event
        {
            Event::SoldierSpawned { id, .. } => id,
            _ => unreachable!(),
        };
        wound_event_id(&world.apply(Command::InflictWound {
            patient,
            wound: WoundSpec {
                trauma: 0,
                bleeding_per_second: 7,
                shock: 0,
            },
        }));
        if hot {
            assert!(world
                .apply(Command::SetRegionHot { cell: 0, hot: true })
                .error
                .is_none());
        }
        let first = world.apply(Command::AdvanceTo { target: 1 });
        let c = world.casualty_state(patient).unwrap();
        assert_eq!((c.blood, c.shock, c.shock_remainder), (4_993, 0, 7));
        assert!(!c.incapacitated);
        assert!(!first
            .events
            .iter()
            .any(|e| matches!(e.event, Event::SoldierDied { .. })));
        let outcome = world.apply(Command::AdvanceTo { target: 715 });
        let deaths: Vec<_> = outcome
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::SoldierDied { .. }))
            .cloned()
            .collect();
        assert_eq!(deaths.len(), 1);
        assert_eq!(deaths[0].at, 715);
        assert!(matches!(
            deaths[0].event,
            Event::SoldierDied {
                cause: DeathCause::Hemorrhage,
                ..
            }
        ));
        (
            world.casualty_state(patient).unwrap(),
            world.soldier(patient).unwrap().living.life,
            deaths,
        )
    }
    let cold = fixture(false);
    let hot = fixture(true);
    assert_eq!(cold, hot);
    assert_eq!(
        (cold.0.blood, cold.0.shock, cold.0.shock_remainder),
        (0, 500, 5)
    );
}

#[test]
fn wound_added_at_nonzero_clock_never_bleeds_retroactively() {
    let mut world = World::new(72);
    let patient = spawn(&mut world, Role::Rifle, 0);
    wound_event_id(&world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 7,
            shock: 0,
        },
    }));
    world.apply(Command::AdvanceTo { target: 5 });
    wound_event_id(&world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 11,
            shock: 0,
        },
    }));
    let at_five = world.casualty_state(patient).unwrap();
    assert_eq!(
        (at_five.blood, at_five.shock, at_five.shock_remainder),
        (4_965, 3, 5)
    );
    world.apply(Command::AdvanceTo { target: 6 });
    let at_six = world.casualty_state(patient).unwrap();
    assert_eq!(
        (at_six.blood, at_six.shock, at_six.shock_remainder),
        (4_947, 5, 3)
    );
    assert_eq!(world.wounds_of(patient).len(), 2);
}

#[test]
fn same_timestamp_hemorrhage_defeats_completion_and_cleans_relationship() {
    let mut world = World::new(73);
    let medic = spawn(&mut world, Role::Medic, HEMOSTATIC_COST);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let wound = wound_event_id(&world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 500,
            shock: 0,
        },
    }));
    let started = world.apply(Command::StartTreatment {
        medic,
        patient,
        wound: Some(wound),
        kind: TreatmentKind::Hemostatic,
    });
    let treatment = match started.events[0].event {
        Event::TreatmentStarted { id, .. } => id,
        _ => panic!(),
    };
    let outcome = world.apply(Command::AdvanceTo {
        target: HEMOSTATIC_DURATION,
    });
    let relevant: Vec<_> = outcome
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.event,
                Event::SoldierDied { .. }
                    | Event::TreatmentInterrupted { .. }
                    | Event::TreatmentCompleted { .. }
            )
        })
        .map(|e| e.event)
        .collect();
    assert!(
        matches!(relevant.as_slice(), [Event::SoldierDied { cause: DeathCause::Hemorrhage, .. }, Event::TreatmentInterrupted { id, reason: InterruptionReason::PatientDied }] if *id == treatment)
    );
    assert!(matches!(
        world.treatment(treatment).unwrap().status,
        TreatmentStatus::Interrupted {
            at: HEMOSTATIC_DURATION,
            reason: InterruptionReason::PatientDied
        }
    ));
    assert!(!world.wound(wound).unwrap().controlled);
    let restart = world.apply(Command::StartTreatment {
        medic,
        patient,
        wound: Some(wound),
        kind: TreatmentKind::Hemostatic,
    });
    assert_eq!(restart.error, Some(SimError::InvalidTreatment));
}

#[test]
fn sparse_boundary_across_empty_hot_cell_keeps_snapshot_accounting_valid() {
    let mut world = World::new(74);
    assert!(world
        .apply(Command::SetRegionHot {
            cell: 99,
            hot: true
        })
        .error
        .is_none());
    let patient = spawn(&mut world, Role::Rifle, 0);
    wound_event_id(&world.apply(Command::InflictWound {
        patient,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 100,
            shock: 0,
        },
    }));
    assert!(world
        .apply(Command::AdvanceTo { target: 50 })
        .error
        .is_none());
    assert_eq!(world.hot_cell(99).unwrap().fixed_steps, 50);
    let bytes = world.snapshot();
    assert_eq!(World::from_snapshot(&bytes).unwrap().snapshot(), bytes);
}
