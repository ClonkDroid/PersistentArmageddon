# ADR 0003: deterministic casualty care

## Status
Accepted for M1.2.

## Model
Each wound has a world-allocated monotonic `WoundId`, owner, creation time, immediate trauma, integer bleeding rate, shock contribution, and control/healing flags. `LivingState.health` remains authoritative. Casualty physiology uses 5,000 integer blood units and 0..1,000 shock; shock 700 or one-third blood incapacitates. Trauma zero health, zero blood, and shock 1,000 cause immediate trauma, hemorrhage, and traumatic-shock death respectively.

Uncontrolled wound rates add with checked integer arithmetic. At each due boundary blood loss precedes shock and death; entity order precedes treatment-ID order, so death interrupts a same-time completion. Controlled wounds cease bleeding. Bleeding-derived shock is one unit per ten blood units lost. A persistent 0..9 numerator remainder makes analytical intervals exactly equivalent to repeated one-second steps for every integer bleeding rate and is encoded in snapshot v7. Shock treatment subtracts 300 shock and begins recovery; it never controls a wound.

## Treatment and conservation
Hemostasis costs one medical unit and lasts 10 seconds. Shock care costs two and lasts 15 seconds. Supplies are removed and recorded as consumed exactly once when treatment starts. Completion provides the benefit and releases both endpoints. Interruption releases both endpoints without refund. Medics come from the ordered `(faction, cell)` role index and the lowest eligible ID wins; selection never scans the soldier store.

Medical accounting is `sourced = carried + consumed + lost`. Spawn is a source and removal is a loss. Snapshot v7 persists physiology, wounds, treatments, allocators, and ledgers; restore rebuilds membership and medic indexes and rejects invalid relationships and trailing bytes.

## Fidelity and limitations
Casualties retain individual authority in hot and cold cells and treatment clocks are absolute, so fidelity changes do not restart work. This increment accepts typed wounds; it is not weapon simulation, tactical movement, AI, or complete combat/M1.
