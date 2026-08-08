use sim_core::*;

fn spawn(world: &mut World, role: Role, medical: u32) -> EntityId {
    let outcome = world.apply(Command::SpawnSoldier {
        spec: SoldierSpec {
            role,
            inventory: Inventory {
                medical,
                ..Inventory::default()
            },
            ..SoldierSpec::default()
        },
    });
    match outcome.events.as_slice() {
        [TimedEvent {
            event: Event::SoldierSpawned { id, .. },
            ..
        }] => *id,
        events => panic!("unexpected spawn events: {events:?}"),
    }
}

fn inflict(world: &mut World, patient: EntityId, wound: WoundSpec) -> WoundId {
    let outcome = world.apply(Command::InflictWound { patient, wound });
    assert_eq!(outcome.error, None);
    match outcome.events[0].event {
        Event::WoundInflicted { id, .. } => id,
        event => panic!("unexpected wound event: {event:?}"),
    }
}

fn start_hemostasis(
    world: &mut World,
    medic: EntityId,
    patient: EntityId,
    wound: WoundId,
) -> TreatmentId {
    let outcome = world.apply(Command::StartTreatment {
        medic,
        patient,
        wound: Some(wound),
        kind: TreatmentKind::Hemostatic,
    });
    assert_eq!(outcome.error, None);
    match outcome.events.as_slice() {
        [TimedEvent {
            event: Event::TreatmentStarted { id, .. },
            ..
        }] => *id,
        events => panic!("unexpected treatment-start events: {events:?}"),
    }
}

fn recovery_world(seed: u64, hot: bool, wound: WoundSpec) -> (World, EntityId, WoundId) {
    let mut world = World::new(seed);
    let medic = spawn(&mut world, Role::Medic, HEMOSTATIC_COST);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let wound_id = inflict(&mut world, patient, wound);
    start_hemostasis(&mut world, medic, patient, wound_id);
    if hot {
        assert_eq!(
            world
                .apply(Command::SetRegionHot { cell: 0, hot: true })
                .error,
            None
        );
    }
    let completed = world.apply(Command::AdvanceTo {
        target: HEMOSTATIC_DURATION,
    });
    assert_eq!(completed.error, None);
    assert_eq!(
        completed.events,
        vec![
            TimedEvent {
                at: 10,
                event: Event::TreatmentCompleted {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    kind: TreatmentKind::Hemostatic,
                },
            },
            TimedEvent {
                at: 10,
                event: Event::RecoveryChanged {
                    id: patient,
                    before: false,
                    after: true,
                    next_at: Some(15),
                },
            },
            TimedEvent {
                at: 10,
                event: Event::TimeAdvanced {
                    from: 0,
                    to: 10,
                    hot_cells_stepped: u64::from(hot),
                    fixed_steps_per_hot_cell: if hot { 10 } else { 0 },
                },
            },
        ]
    );
    (world, patient, wound_id)
}

#[test]
fn gate_b_acceptance_01_recovery_boundary_minus_exact_plus_one() {
    let (base, patient, wound) = recovery_world(
        101,
        false,
        WoundSpec {
            trauma: 50,
            bleeding_per_second: 2,
            shock: 100,
        },
    );
    let checkpoint = base.snapshot();
    for (target, expected_events) in [
        (
            14,
            vec![TimedEvent {
                at: 14,
                event: Event::TimeAdvanced {
                    from: 10,
                    to: 14,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0,
                },
            }],
        ),
        (
            15,
            vec![
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 4_980,
                        blood_after: 5_000,
                        shock_before: 102,
                        shock_after: 52,
                        health_before: 950,
                        health_after: 975,
                    },
                },
                TimedEvent {
                    at: 15,
                    event: Event::TimeAdvanced {
                        from: 10,
                        to: 15,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0,
                    },
                },
            ],
        ),
        (
            16,
            vec![
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 4_980,
                        blood_after: 5_000,
                        shock_before: 102,
                        shock_after: 52,
                        health_before: 950,
                        health_after: 975,
                    },
                },
                TimedEvent {
                    at: 16,
                    event: Event::TimeAdvanced {
                        from: 10,
                        to: 16,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0,
                    },
                },
            ],
        ),
    ] {
        let mut branch = World::from_snapshot(&checkpoint).unwrap();
        let outcome = branch.apply(Command::AdvanceTo { target });
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.events, expected_events);
        let casualty = branch.casualty_state(patient).unwrap();
        let soldier = branch.soldier(patient).unwrap();
        let wound_state = branch.wound(wound).unwrap();
        if target == 14 {
            assert_eq!((casualty.blood, casualty.shock), (4_980, 102));
            assert_eq!(soldier.living.health, 950);
            assert_eq!(casualty.recovery_next_at, Some(15));
        } else {
            assert_eq!((casualty.blood, casualty.shock), (5_000, 52));
            assert_eq!(soldier.living.health, 975);
            assert_eq!(casualty.recovery_next_at, Some(20));
        }
        assert!(!casualty.incapacitated);
        assert!(casualty.recovering);
        assert!(wound_state.controlled);
        assert!(!wound_state.healed);
        assert_eq!(wound_state.spec.bleeding_per_second, 2);
    }
}

#[test]
fn gate_b_acceptance_02_independent_hot_and_cold_large_leap_recovery() {
    let spec = WoundSpec {
        trauma: 100,
        bleeding_per_second: 10,
        shock: 200,
    };
    for (seed, hot) in [(202, false), (203, true)] {
        let (mut world, patient, wound) = recovery_world(seed, hot, spec);
        let outcome = world.apply(Command::AdvanceTo { target: 35 });
        assert_eq!(outcome.error, None);
        assert_eq!(
            outcome.events,
            vec![
                TimedEvent {
                    at: 15,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 4_900,
                        blood_after: 5_000,
                        shock_before: 210,
                        shock_after: 160,
                        health_before: 900,
                        health_after: 925
                    }
                },
                TimedEvent {
                    at: 20,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 5_000,
                        blood_after: 5_000,
                        shock_before: 160,
                        shock_after: 110,
                        health_before: 925,
                        health_after: 950
                    }
                },
                TimedEvent {
                    at: 25,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 5_000,
                        blood_after: 5_000,
                        shock_before: 110,
                        shock_after: 60,
                        health_before: 950,
                        health_after: 975
                    }
                },
                TimedEvent {
                    at: 30,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 5_000,
                        blood_after: 5_000,
                        shock_before: 60,
                        shock_after: 10,
                        health_before: 975,
                        health_after: 1_000
                    }
                },
                TimedEvent {
                    at: 35,
                    event: Event::RecoveryTicked {
                        id: patient,
                        blood_before: 5_000,
                        blood_after: 5_000,
                        shock_before: 10,
                        shock_after: 0,
                        health_before: 1_000,
                        health_after: 1_000
                    }
                },
                TimedEvent {
                    at: 35,
                    event: Event::WoundHealed { id: wound, patient }
                },
                TimedEvent {
                    at: 35,
                    event: Event::RecoveryChanged {
                        id: patient,
                        before: true,
                        after: false,
                        next_at: None
                    }
                },
                TimedEvent {
                    at: 35,
                    event: Event::TimeAdvanced {
                        from: 10,
                        to: 35,
                        hot_cells_stepped: u64::from(hot),
                        fixed_steps_per_hot_cell: if hot { 25 } else { 0 }
                    }
                },
            ]
        );
        let casualty = world.casualty_state(patient).unwrap();
        assert_eq!((casualty.blood, casualty.shock), (5_000, 0));
        assert_eq!(world.soldier(patient).unwrap().living.health, 1_000);
        assert!(!casualty.incapacitated);
        assert!(!casualty.recovering);
        assert_eq!(casualty.recovery_next_at, None);
        assert!(world.wound(wound).unwrap().healed);
    }
}

#[test]
fn gate_b_acceptance_03_two_wound_final_control_gates_distinct_healing() {
    let mut world = World::new(303);
    let medic = spawn(&mut world, Role::Medic, 2);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let first = inflict(
        &mut world,
        patient,
        WoundSpec {
            trauma: 100,
            bleeding_per_second: 1,
            shock: 100,
        },
    );
    let second = inflict(
        &mut world,
        patient,
        WoundSpec {
            trauma: 100,
            bleeding_per_second: 1,
            shock: 100,
        },
    );
    assert_eq!((first, second), (WoundId(0), WoundId(1)));

    start_hemostasis(&mut world, medic, patient, first);
    let first_done = world.apply(Command::AdvanceTo { target: 10 });
    assert_eq!(
        first_done.events,
        vec![
            TimedEvent {
                at: 10,
                event: Event::TreatmentCompleted {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    kind: TreatmentKind::Hemostatic
                }
            },
            TimedEvent {
                at: 10,
                event: Event::TimeAdvanced {
                    from: 0,
                    to: 10,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            }
        ]
    );
    assert!(world.wound(first).unwrap().controlled);
    assert!(!world.wound(second).unwrap().controlled);
    assert!(!world.casualty_state(patient).unwrap().recovering);
    assert_eq!(
        world.casualty_state(patient).unwrap().recovery_next_at,
        None
    );

    start_hemostasis(&mut world, medic, patient, second);
    let second_done = world.apply(Command::AdvanceTo { target: 20 });
    assert_eq!(
        second_done.events,
        vec![
            TimedEvent {
                at: 20,
                event: Event::TreatmentCompleted {
                    id: TreatmentId(1),
                    medic,
                    patient,
                    kind: TreatmentKind::Hemostatic
                }
            },
            TimedEvent {
                at: 20,
                event: Event::RecoveryChanged {
                    id: patient,
                    before: false,
                    after: true,
                    next_at: Some(25)
                }
            },
            TimedEvent {
                at: 20,
                event: Event::TimeAdvanced {
                    from: 10,
                    to: 20,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            },
        ]
    );
    assert_eq!(
        world.casualty_state(patient).unwrap().recovery_next_at,
        Some(25)
    );

    let finished = world.apply(Command::AdvanceTo { target: 60 });
    assert_eq!(
        finished.events,
        vec![
            TimedEvent {
                at: 25,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 4_970,
                    blood_after: 5_000,
                    shock_before: 203,
                    shock_after: 153,
                    health_before: 800,
                    health_after: 825
                }
            },
            TimedEvent {
                at: 30,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 153,
                    shock_after: 103,
                    health_before: 825,
                    health_after: 850
                }
            },
            TimedEvent {
                at: 35,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 103,
                    shock_after: 53,
                    health_before: 850,
                    health_after: 875
                }
            },
            TimedEvent {
                at: 40,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 53,
                    shock_after: 3,
                    health_before: 875,
                    health_after: 900
                }
            },
            TimedEvent {
                at: 45,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 3,
                    shock_after: 0,
                    health_before: 900,
                    health_after: 925
                }
            },
            TimedEvent {
                at: 50,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 0,
                    shock_after: 0,
                    health_before: 925,
                    health_after: 950
                }
            },
            TimedEvent {
                at: 55,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 0,
                    shock_after: 0,
                    health_before: 950,
                    health_after: 975
                }
            },
            TimedEvent {
                at: 60,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 5_000,
                    blood_after: 5_000,
                    shock_before: 0,
                    shock_after: 0,
                    health_before: 975,
                    health_after: 1_000
                }
            },
            TimedEvent {
                at: 60,
                event: Event::WoundHealed { id: first, patient }
            },
            TimedEvent {
                at: 60,
                event: Event::WoundHealed {
                    id: second,
                    patient
                }
            },
            TimedEvent {
                at: 60,
                event: Event::RecoveryChanged {
                    id: patient,
                    before: true,
                    after: false,
                    next_at: None
                }
            },
            TimedEvent {
                at: 60,
                event: Event::TimeAdvanced {
                    from: 20,
                    to: 60,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            },
        ]
    );
    assert!(world.wound(first).unwrap().healed);
    assert!(world.wound(second).unwrap().healed);
}

#[test]
fn gate_b_acceptance_04_shock_care_while_bleeding_never_starts_recovery() {
    let mut world = World::new(404);
    let medic = spawn(&mut world, Role::Medic, SHOCK_TREATMENT_COST);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let wound = inflict(
        &mut world,
        patient,
        WoundSpec {
            trauma: 10,
            bleeding_per_second: 2,
            shock: 400,
        },
    );
    let before = world.wound(wound).unwrap();
    let started = world.apply(Command::StartTreatment {
        medic,
        patient,
        wound: None,
        kind: TreatmentKind::Shock,
    });
    assert_eq!(
        started.events,
        vec![TimedEvent {
            at: 0,
            event: Event::TreatmentStarted {
                id: TreatmentId(0),
                medic,
                patient,
                wound: None,
                kind: TreatmentKind::Shock,
                completes_at: 15,
                consumed: 2
            }
        }]
    );
    let completed = world.apply(Command::AdvanceTo { target: 15 });
    assert_eq!(
        completed.events,
        vec![
            TimedEvent {
                at: 15,
                event: Event::TreatmentCompleted {
                    id: TreatmentId(0),
                    medic,
                    patient,
                    kind: TreatmentKind::Shock
                }
            },
            TimedEvent {
                at: 15,
                event: Event::TimeAdvanced {
                    from: 0,
                    to: 15,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            }
        ]
    );
    assert_eq!(world.wound(wound).unwrap(), before);
    let casualty = world.casualty_state(patient).unwrap();
    assert_eq!((casualty.blood, casualty.shock), (4_970, 103));
    assert!(!casualty.recovering);
    assert_eq!(casualty.recovery_next_at, None);
    let later = world.apply(Command::AdvanceTo { target: 20 });
    assert_eq!(
        later.events,
        vec![TimedEvent {
            at: 20,
            event: Event::TimeAdvanced {
                from: 15,
                to: 20,
                hot_cells_stepped: 0,
                fixed_steps_per_hot_cell: 0
            }
        }]
    );
    assert_eq!(
        (
            world.casualty_state(patient).unwrap().blood,
            world.casualty_state(patient).unwrap().shock
        ),
        (4_960, 104)
    );
    assert!(!world.wound(wound).unwrap().controlled);
}

#[test]
fn gate_b_acceptance_05_new_wound_cancels_and_restarts_absolute_recovery() {
    let (mut world, patient, first) = recovery_world(
        505,
        false,
        WoundSpec {
            trauma: 50,
            bleeding_per_second: 2,
            shock: 100,
        },
    );
    let medic = spawn(&mut world, Role::Medic, HEMOSTATIC_COST);
    assert_eq!(
        world.apply(Command::AdvanceTo { target: 12 }).events,
        vec![TimedEvent {
            at: 12,
            event: Event::TimeAdvanced {
                from: 10,
                to: 12,
                hot_cells_stepped: 0,
                fixed_steps_per_hot_cell: 0
            }
        }]
    );
    let second_spec = WoundSpec {
        trauma: 10,
        bleeding_per_second: 1,
        shock: 10,
    };
    let inflicted = world.apply(Command::InflictWound {
        patient,
        wound: second_spec,
    });
    assert_eq!(
        inflicted.events,
        vec![
            TimedEvent {
                at: 12,
                event: Event::WoundInflicted {
                    id: WoundId(1),
                    patient,
                    wound: second_spec
                }
            },
            TimedEvent {
                at: 12,
                event: Event::RecoveryChanged {
                    id: patient,
                    before: true,
                    after: false,
                    next_at: None
                }
            },
        ]
    );
    let second = WoundId(1);
    assert!(!world.casualty_state(patient).unwrap().recovering);
    start_hemostasis(&mut world, medic, patient, second);
    let restarted = world.apply(Command::AdvanceTo { target: 22 });
    assert_eq!(
        restarted.events,
        vec![
            TimedEvent {
                at: 22,
                event: Event::TreatmentCompleted {
                    id: TreatmentId(1),
                    medic,
                    patient,
                    kind: TreatmentKind::Hemostatic
                }
            },
            TimedEvent {
                at: 22,
                event: Event::RecoveryChanged {
                    id: patient,
                    before: false,
                    after: true,
                    next_at: Some(27)
                }
            },
            TimedEvent {
                at: 22,
                event: Event::TimeAdvanced {
                    from: 12,
                    to: 22,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            },
        ]
    );
    assert_eq!(
        world.casualty_state(patient).unwrap().recovery_next_at,
        Some(27)
    );
    assert_eq!(
        (
            world.casualty_state(patient).unwrap().blood,
            world.casualty_state(patient).unwrap().shock
        ),
        (4_970, 113)
    );
    assert!(world.wound(first).unwrap().controlled);
    assert!(world.wound(second).unwrap().controlled);
    let tick = world.apply(Command::AdvanceTo { target: 27 });
    assert_eq!(
        tick.events,
        vec![
            TimedEvent {
                at: 27,
                event: Event::RecoveryTicked {
                    id: patient,
                    blood_before: 4_970,
                    blood_after: 5_000,
                    shock_before: 113,
                    shock_after: 63,
                    health_before: 940,
                    health_after: 965
                }
            },
            TimedEvent {
                at: 27,
                event: Event::TimeAdvanced {
                    from: 22,
                    to: 27,
                    hot_cells_stepped: 0,
                    fixed_steps_per_hot_cell: 0
                }
            }
        ]
    );
}

fn active_hemostasis_world(seed: u64) -> (World, EntityId, EntityId, WoundId, TreatmentId) {
    let mut world = World::new(seed);
    let medic = spawn(&mut world, Role::Medic, HEMOSTATIC_COST);
    let patient = spawn(&mut world, Role::Rifle, 0);
    let wound = inflict(
        &mut world,
        patient,
        WoundSpec {
            trauma: 10,
            bleeding_per_second: 1,
            shock: 0,
        },
    );
    let treatment = start_hemostasis(&mut world, medic, patient, wound);
    (world, medic, patient, wound, treatment)
}

#[test]
fn gate_b_acceptance_06_immediate_wound_consequences_and_endpoint_interruptions() {
    // Immediate trauma death while the casualty is an active patient.
    let (mut trauma, trauma_medic, trauma_patient, trauma_wound, trauma_treatment) =
        active_hemostasis_world(6_061);
    assert_eq!(trauma.soldier(trauma_medic).unwrap().inventory.medical, 0);
    let outcome = trauma.apply(Command::InflictWound {
        patient: trauma_patient,
        wound: WoundSpec {
            trauma: 1_000,
            bleeding_per_second: 0,
            shock: 0,
        },
    });
    assert_eq!(outcome.error, None);
    assert_eq!(
        outcome.events,
        vec![
            TimedEvent {
                at: 0,
                event: Event::WoundInflicted {
                    id: WoundId(1),
                    patient: trauma_patient,
                    wound: WoundSpec {
                        trauma: 1_000,
                        bleeding_per_second: 0,
                        shock: 0
                    }
                }
            },
            TimedEvent {
                at: 0,
                event: Event::SoldierDied {
                    id: trauma_patient,
                    cause: DeathCause::ImmediateTrauma,
                    health_before: 990
                }
            },
            TimedEvent {
                at: 0,
                event: Event::TreatmentInterrupted {
                    id: trauma_treatment,
                    reason: InterruptionReason::PatientDied
                }
            },
        ]
    );
    assert_eq!(
        trauma.soldier(trauma_patient).unwrap().living.life,
        LifeState::Dead {
            at: 0,
            cause: DeathCause::ImmediateTrauma
        }
    );
    assert_eq!(trauma.soldier(trauma_patient).unwrap().living.health, 0);
    assert!(!trauma.casualty_state(trauma_patient).unwrap().recovering);
    assert_eq!(
        trauma.treatment(trauma_treatment).unwrap().status,
        TreatmentStatus::Interrupted {
            at: 0,
            reason: InterruptionReason::PatientDied
        }
    );
    assert!(!trauma.wound(trauma_wound).unwrap().controlled);
    let later = trauma.apply(Command::AdvanceTo { target: 20 });
    assert!(!later.events.iter().any(
        |e| matches!(e.event, Event::TreatmentCompleted { id, .. } if id == trauma_treatment)
    ));

    // Immediate traumatic shock death while the casualty is the active medic.
    let (mut shock, shock_medic, _shock_patient, shock_wound, shock_treatment) =
        active_hemostasis_world(6_062);
    let outcome = shock.apply(Command::InflictWound {
        patient: shock_medic,
        wound: WoundSpec {
            trauma: 0,
            bleeding_per_second: 0,
            shock: 1_000,
        },
    });
    assert_eq!(outcome.error, None);
    assert_eq!(
        outcome.events,
        vec![
            TimedEvent {
                at: 0,
                event: Event::WoundInflicted {
                    id: WoundId(1),
                    patient: shock_medic,
                    wound: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 0,
                        shock: 1_000
                    }
                }
            },
            TimedEvent {
                at: 0,
                event: Event::SoldierDied {
                    id: shock_medic,
                    cause: DeathCause::TraumaticShock,
                    health_before: 1_000
                }
            },
            TimedEvent {
                at: 0,
                event: Event::TreatmentInterrupted {
                    id: shock_treatment,
                    reason: InterruptionReason::MedicDied
                }
            },
        ]
    );
    assert_eq!(
        shock.soldier(shock_medic).unwrap().living.life,
        LifeState::Dead {
            at: 0,
            cause: DeathCause::TraumaticShock
        }
    );
    assert_eq!(
        shock.treatment(shock_treatment).unwrap().status,
        TreatmentStatus::Interrupted {
            at: 0,
            reason: InterruptionReason::MedicDied
        }
    );
    assert!(!shock.wound(shock_wound).unwrap().controlled);
    assert!(!shock.casualty_state(shock_medic).unwrap().recovering);

    // A marching casualty is forced idle at the instant incapacity is crossed.
    let mut marching = World::new(6_063);
    let marcher = spawn(&mut marching, Role::Rifle, 0);
    assert_eq!(
        marching
            .apply(Command::SetActivity {
                id: marcher,
                activity: Activity::March
            })
            .error,
        None
    );
    let outcome = marching.apply(Command::InflictWound {
        patient: marcher,
        wound: WoundSpec {
            trauma: 1,
            bleeding_per_second: 0,
            shock: 700,
        },
    });
    assert_eq!(
        outcome.events,
        vec![
            TimedEvent {
                at: 0,
                event: Event::WoundInflicted {
                    id: WoundId(0),
                    patient: marcher,
                    wound: WoundSpec {
                        trauma: 1,
                        bleeding_per_second: 0,
                        shock: 700
                    }
                }
            },
            TimedEvent {
                at: 0,
                event: Event::ActivityChanged {
                    id: marcher,
                    before: Activity::March,
                    after: Activity::Idle,
                    forced: true
                }
            },
        ]
    );
    assert!(marching.casualty_state(marcher).unwrap().incapacitated);
    assert_eq!(
        marching.soldier(marcher).unwrap().living.activity,
        Activity::Idle
    );

    for (seed, wound_medic, reason) in [
        (6_064, true, InterruptionReason::Ineligible),
        (6_065, false, InterruptionReason::Ineligible),
    ] {
        let (mut world, medic, patient, treated_wound, treatment) = active_hemostasis_world(seed);
        let endpoint = if wound_medic { medic } else { patient };
        let outcome = world.apply(Command::InflictWound {
            patient: endpoint,
            wound: WoundSpec {
                trauma: 1,
                bleeding_per_second: 0,
                shock: 700,
            },
        });
        assert_eq!(outcome.error, None);
        assert_eq!(
            outcome.events,
            vec![
                TimedEvent {
                    at: 0,
                    event: Event::WoundInflicted {
                        id: WoundId(1),
                        patient: endpoint,
                        wound: WoundSpec {
                            trauma: 1,
                            bleeding_per_second: 0,
                            shock: 700
                        }
                    }
                },
                TimedEvent {
                    at: 0,
                    event: Event::TreatmentInterrupted {
                        id: treatment,
                        reason
                    }
                },
            ]
        );
        assert_eq!(
            world.treatment(treatment).unwrap().status,
            TreatmentStatus::Interrupted { at: 0, reason }
        );
        assert_eq!(world.soldier(medic).unwrap().inventory.medical, 0);
        assert!(!world.wound(treated_wound).unwrap().controlled);
        let later = world.apply(Command::AdvanceTo { target: 20 });
        assert!(!later
            .events
            .iter()
            .any(|e| matches!(e.event, Event::TreatmentCompleted { id, .. } if id == treatment)));
    }
}

#[test]
fn gate_b_acceptance_08_automatic_incapacity_interrupts_before_completion() {
    for (seed, wound_medic) in [(6_081, true), (6_082, false)] {
        let (mut world, medic, patient, treated_wound, treatment) = active_hemostasis_world(seed);
        let endpoint = if wound_medic { medic } else { patient };
        let inflicted = world.apply(Command::InflictWound {
            patient: endpoint,
            wound: WoundSpec {
                trauma: 0,
                bleeding_per_second: 1_000,
                shock: 0,
            },
        });
        assert_eq!(
            inflicted.events,
            vec![TimedEvent {
                at: 0,
                event: Event::WoundInflicted {
                    id: WoundId(1),
                    patient: endpoint,
                    wound: WoundSpec {
                        trauma: 0,
                        bleeding_per_second: 1_000,
                        shock: 0
                    }
                }
            }]
        );
        let advanced = world.apply(Command::AdvanceTo { target: 10 });
        assert_eq!(advanced.error, None);
        assert_eq!(
            advanced.events,
            vec![
                TimedEvent {
                    at: 4,
                    event: Event::TreatmentInterrupted {
                        id: treatment,
                        reason: InterruptionReason::Ineligible
                    }
                },
                TimedEvent {
                    at: 5,
                    event: Event::SoldierDied {
                        id: endpoint,
                        cause: DeathCause::Hemorrhage,
                        health_before: if wound_medic { 1_000 } else { 990 }
                    }
                },
                TimedEvent {
                    at: 10,
                    event: Event::TimeAdvanced {
                        from: 0,
                        to: 10,
                        hot_cells_stepped: 0,
                        fixed_steps_per_hot_cell: 0
                    }
                },
            ]
        );
        assert_eq!(
            world.treatment(treatment).unwrap().status,
            TreatmentStatus::Interrupted {
                at: 4,
                reason: InterruptionReason::Ineligible
            }
        );
        assert_eq!(world.soldier(medic).unwrap().inventory.medical, 0);
        assert!(!world.wound(treated_wound).unwrap().controlled);
        assert!(!advanced
            .events
            .iter()
            .any(|e| matches!(e.event, Event::TreatmentCompleted { .. })));
        let later = world.apply(Command::AdvanceTo { target: 20 });
        assert!(!later
            .events
            .iter()
            .any(|e| matches!(e.event, Event::TreatmentCompleted { .. })));
    }
}
