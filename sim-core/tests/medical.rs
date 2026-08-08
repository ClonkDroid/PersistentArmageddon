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
