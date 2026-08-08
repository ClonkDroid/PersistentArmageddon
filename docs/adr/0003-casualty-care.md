# ADR 0003: deterministic casualty care

## Status
Accepted for M1.2.

## Model
Each wound has a world-allocated monotonic `WoundId`, owner, creation time, immediate trauma, integer bleeding rate, shock contribution, and control/healing flags. `LivingState.health` remains authoritative. Casualty physiology uses 5,000 integer blood units and 0..1,000 shock; shock 700 or one-third blood incapacitates. Trauma zero health, zero blood, and shock 1,000 cause immediate trauma, hemorrhage, and traumatic-shock death respectively.

Uncontrolled wound rates add with checked integer arithmetic. At each due boundary living needs and casualty physiology are both materialized through the shared timestamp. Living-needs death has precedence over a simultaneous medical death, blood loss precedes shock among medical causes, and exactly one death is emitted before one resulting treatment interruption. Entity order precedes treatment-ID order, so death interrupts a same-time completion. Controlled wounds cease bleeding. Bleeding-derived shock is one unit per ten blood units lost. A persistent 0..9 numerator remainder makes analytical intervals exactly equivalent to repeated one-second steps for every integer bleeding rate and is encoded in snapshot v7. Shock treatment subtracts 300 shock; it never controls a wound. Incapacitation is recomputed from both authoritative thresholds after every physiology mutation: shock at least 700 or blood at most one third of maximum.


Recovery begins only for a living casualty with nonzero blood, sub-fatal shock, no uncontrolled bleeding, and only controlled or healed wounds. It has one persisted next boundary. Every five seconds it restores 100 blood units, removes 50 shock units, and restores 25 points of authoritative health, all with their normal caps. At the first boundary where blood is 5,000, shock is zero, and health is 1,000, every controlled wound is marked healed in stable wound-ID order, recovery ends, and its due boundary is removed. Wound history remains authoritative. A new wound cancels recovery before its consequences are applied. Hot one-second execution and cold sparse execution use this same boundary rule.

## Treatment and conservation
Hemostasis costs one medical unit and lasts 10 seconds. Shock care costs two and lasts 15 seconds. Supplies are removed and recorded as consumed exactly once when treatment starts. Completion provides the benefit and releases both endpoints. Interruption releases both endpoints without refund. Medics come from an exact ordered `(faction, cell, treatment cost)` availability index and the lowest eligible ID wins; dead, marching, incapacitated, busy, and under-supplied medics are absent rather than filtered during traversal. Starting treatment rejects a marching patient atomically. Selection never scans the soldier store.

Immediate wound commands emit causal event lists: wound creation first, then a nonfatal forced-idle transition, a singular death when applicable, recovery cancellation, and one treatment interruption. Fatal trauma takes precedence over initial traumatic shock. Explicit removal emits its treatment interruption before removal and deletes wound and treatment history through per-entity membership indexes; remaining carried medical is an explicit loss.

Medical accounting is `sourced = carried + consumed + lost`. Spawn is a source and removal is a loss. Snapshot v7 persists physiology, wounds, treatments, allocators, and ledgers; restore rebuilds membership and medic indexes and rejects invalid relationships and trailing bytes.

## Fidelity and limitations
Casualties retain individual authority in hot and cold cells and treatment clocks are absolute, so fidelity changes do not restart work. This increment accepts typed wounds; it is not weapon simulation, tactical movement, AI, or complete combat/M1.
