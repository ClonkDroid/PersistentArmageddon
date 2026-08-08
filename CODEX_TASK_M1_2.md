# Codex task M1.2: wounds, treatment, recovery, and medics

Tracking issue: #3  
Parent roadmap: #2  
M1.1 base commit: `ea3758ee1cdce669c66f343b3d9d9b86e7820f29`

## Objective

Extend the M1.1 living-state engine with the first real casualty-care simulation.

After this increment, an authoritative command can inflict one or more persistent wounds on an individual soldier. Wounds cause actual blood loss, shock, impairment, health loss, recovery, and death. Eligible medic entities must be selected deterministically or addressed explicitly, spend their own conserved medical inventory, remain occupied for a real duration, and either complete or interrupt a treatment that changes the patient's authoritative state.

Cold execution may skip uneventful time analytically, but every medical threshold, treatment completion, interruption, recovery transition, and death due by world clock T must execute at its exact deterministic time. Hot execution must step actual indexed members through the same defined one-second semantics. Queries, fidelity changes, snapshots, and replay must never create a different casualty history.

This is M1.2, not combat. Do not add weapon ballistics, armour penetration, hit location, projectiles, tactical movement, pathfinding, line of sight, enemy AI, vehicles, depots, formations above the existing squad shell, communications, graphics, copied lore, or proprietary assets. Until M2, wounds enter only through typed authoritative commands and deterministic test/scenario setup.

## Non-negotiable semantics

### Individual authority and identity

- Preserve one authoritative soldier record per stable `EntityId`; never replace cold casualties or medics with aggregates.
- Introduce a stable, ordered `WoundId` (or equivalent) allocated by the world. IDs must be deterministic across replay and snapshot continuation and must not alias after soldier generation reuse.
- Preserve each wound as an individually queryable authoritative record. Multiple wounds on one soldier must remain distinct; do not collapse them into a single narration string, Boolean, or aggregate counter.
- Introduce a stable, ordered `TreatmentId` (or equivalent) for every started treatment. One active treatment must be addressable for interruption and audit.
- Keep `LivingState.health` as the single health authority. Wounds may change it, but must not create a second independently mutable health field. Preserve the existing `SoldierSpec.health` mirror invariant.
- All authoritative values use integers or fixed point. Results cannot depend on wall time, thread scheduling, hash iteration, floating point, or machine entropy.
- Dead soldiers, their wounds, and their completed/interrupted treatment history remain queryable until explicit soldier removal. Death never automatically despawns or recycles an entity.

### Wound and casualty state

Define and document compact public types equivalent to:

- a wound specification containing immediate trauma, ongoing bleeding, and initial shock contribution;
- a persistent wound record with stable ID, creation time, severity, bleeding/control state, and recovery/healed state;
- per-soldier casualty physiology including remaining blood, shock, impairment/recovery phase, and the shared materialization time required by the canonical transition;
- explicit treatment kind and active-treatment state containing medic, patient, target wound where applicable, start time, completion time, consumed medical quantity, and status/reason.

Exact names, integer units, rates, thresholds, and balanced constants may be chosen by the implementation, but they must be small, public, documented, and exercised at boundary-1/boundary/boundary+1. The rules must provide all of the following observable behavior:

- Inflicting a wound immediately changes the target's authoritative wound/casualty state and may reduce the existing authoritative health.
- Every uncontrolled bleeding wound contributes to actual blood loss over simulated time. Multiple wounds combine with checked arithmetic and remain separately treatable.
- Blood loss and wound severity produce actual shock progression. Document deterministic precedence when trauma, hemorrhage, and shock could kill at the same timestamp.
- Add explicit persistent death causes for immediate trauma, hemorrhage, and traumatic shock while preserving all M1.1 causes.
- Defined impairment thresholds must materially constrain behavior. At minimum, an incapacitated casualty cannot march, act as a medic, start treatment, or continue treating another soldier. A forced activity change must emit the existing complete typed activity event.
- Controlled wounds stop contributing the documented bleeding rate. Shock and blood volume do not reset merely because bleeding stopped.
- Once the documented stabilization conditions are met, a living casualty enters a real recovery phase. Recovery must change actual wound, blood, shock, and/or health values over simulated time until a documented stable/healed boundary; it cannot be a label-only timer.
- New wounds during stabilization or recovery must deterministically worsen state, cancel invalid recovery, and interrupt treatment when the medic or patient becomes ineligible.
- A dead soldier never bleeds further, recovers, receives or provides treatment, consumes additional medical supplies, or emits a second death event.

Add an atomic authoritative wound command accepting a valid living target and bounded wound parameters. It allocates the `WoundId`, materializes the target at the current clock, applies the immediate consequences, emits a complete typed wound event, schedules the exact next boundary, and emits impairment/activity/death events in the documented order. Invalid, dead, stale-generation, zero-effect, out-of-range, or overflowing inputs must fail before any ID, state, event, inventory, ledger, counter, or index mutation.

### Medic selection and treatment action

Implement both paths below so future order/AI layers can choose explicitly while scenarios can request deterministic help:

1. A direct start-treatment command naming medic, patient, wound where applicable, and treatment kind.
2. A request-treatment command naming patient, wound where applicable, and treatment kind; the world selects an eligible medic deterministically.

Selection must consider only authoritative indexed candidates and choose the lowest stable `EntityId` among all eligible candidates. At minimum an eligible medic must be:

- a valid living `Role::Medic` of the patient's faction;
- in the same cell as the patient (M1.2 deliberately has no movement or line-of-sight model);
- not incapacitated, marching, already treating, or already receiving treatment;
- carrying enough medical inventory for the requested treatment.

The implementation must not scan every soldier in the world to find a medic. Maintain an exact role/cell/faction availability index or a rigorously equivalent bounded candidate index, including reverse cleanup on spawn, removal, death, role eligibility changes, treatment start/end, fidelity transition, and restore.

At most one treatment may actively occupy a medic and at most one treatment may target a patient. Self-treatment is invalid in M1.2. Starting treatment validates the complete relationship atomically, allocates one treatment ID, removes the documented medical quantity from the selected medic exactly once, increments the medical-consumption ledger, marks both entities busy, installs one exact completion boundary, and emits a typed start event containing all IDs, kind, start/completion time, and inventory before/after or consumed quantity.

Provide at least:

- hemostatic treatment that changes the target wound's actual bleeding/control state on completion; and
- shock treatment that changes actual shock/stabilization state but cannot silently close an uncontrolled wound.

Treatment durations and costs must be positive documented integer constants. A treatment produces no benefit before completion. Completion must verify the still-valid relationship, apply the real state change exactly once, release medic and patient, update all indexes/due times, and emit a complete typed event.

Add an explicit interrupt-treatment command. Also interrupt automatically when medic or patient dies, is explicitly removed, becomes ineligible/incapacitated, or another defined state transition makes the action impossible. Interruption records a deterministic reason, releases both sides, removes the completion entry, and emits exactly one event. Medical supplies opened/used at treatment start remain consumed after interruption; they are never silently refunded. A failed start consumes nothing, and completion/interruption races cannot double-consume, double-release, or apply treatment after death.

Activity commands involving an active or incapacitated participant must have one documented atomic rule. Prefer rejecting an attempt to march until the treatment is explicitly interrupted; do not silently leave a medic treating while marching.

### One canonical transition and sparse cold advancement

Extend the M1.1 semantic oracle rather than creating an independent medical clock.

- One canonical one-second transition result must cover needs, wounds, blood loss, shock, impairment, recovery, inventory/accounting deltas, activity effects, life/death, and typed automatic events.
- The analytical cold path must be exactly equivalent to repeated reference steps for the defined state and event stream.
- Cold work must be bounded by actual medical/living boundaries such as threshold crossings, death, treatment completion, recovery ticks/phase changes, ration use, and forced activity—not by elapsed seconds times all cold soldiers.
- Extend the existing reverse due index, or replace it with one rigorously unified automatic-boundary index. Do not leave separate living and medical schedulers that can advance the same soldier twice or disagree about event order.
- Treatment completion must have a reverse entry and must touch only its medic and patient. Starting, interrupting, completing, dying, removal, wound infliction, inventory change, fidelity transition, and restore must remove or replace all affected entries without stale or duplicate IDs.
- Queries may project a private copy to the world clock but may not execute consequences, select medics, complete treatments, mutate an index, change counters, alter snapshot bytes, or change the digest.
- Hot cells continue to step their actual indexed living members once per second. Their medical state must use the same transition oracle and produce the same defined state/events as cold analytical execution at every boundary.
- Fidelity changes materialize exact casualty/treatment state at the boundary and cannot heal, duplicate blood loss, restart duration, refund supplies, or interrupt a logically valid same-cell treatment merely because rendering fidelity changed.

At any automatic timestamp use this documented global order:

1. Apply per-soldier living/medical transitions in stable `EntityId` order, including impairment and death.
2. Apply automatic treatment interruptions caused by those results, then valid treatment completions, in stable `TreatmentId` order. If the patient or medic died at that timestamp, interruption wins and no benefit is applied.
3. Execute scheduled commands at that timestamp in stable schedule-ID order, preserving M1.1 committed-prefix behavior.

Immediate commands at the current clock must first materialize every entity they touch. Events emitted within one command must use a documented causal order and stay stable across hot/cold execution and restore.

### Atomicity and rollback

Extend M1.1's touched-record transaction journal; do not clone `World` for advance, fidelity, treatment, or wound commands.

- One automatic segment journal must cover every soldier/wound/treatment record, inventory, due/reverse entry, availability relationship, ledger, counter, ID allocator, and emitted event it can touch.
- Any overflow or invariant failure rolls the entire current automatic segment back exactly, including earlier internal medical boundaries in that segment. Earlier externally committed scheduled segments remain the valid prefix.
- Direct multi-entity commands are all-or-nothing. A failed treatment start, wound application, interruption, despawn cleanup, or fidelity change leaves snapshot bytes and digest unchanged.
- Journal/candidate work must be proportional to due records, active treatment endpoints, and indexed hot members reached by the segment, never total world population.

### Medical conservation

Extend authoritative resource accounting so:

`sourced medical = currently carried medical + consumed medical + explicit medical losses`.

- Soldier spawn/loadout is an explicit medical source.
- Explicit removal is an explicit loss of all remaining carried medical inventory.
- Treatment start is the only M1.2 medical consumption point and increments consumed totals exactly once.
- Selection reads inventory without reserving or consuming it; only the atomically successful start changes inventory.
- Treatment completion, interruption, death, fidelity changes, queries, snapshot, and restore cannot create, refund, or consume additional medical inventory unless a typed rule explicitly says so.
- Checked ledger overflow must fail atomically. Snapshot and digest include the complete medical ledger.
- Preserve the existing food/water conservation and ammunition/supply invariants.

### Lifecycle, snapshot, replay, and protocol

- Removal of a medic or patient must deterministically interrupt active treatment before relationship cleanup, remove all wound/treatment/due/availability entries owned by the entity, and account remaining carried medical as loss. Generation reuse must never inherit wounds, shock, busy state, recovery, or stale treatment completion.
- Snapshot format becomes v7. Supporting v6 restore is not required in M1.2; reject unsupported versions clearly.
- Encode every casualty field, wound and treatment record, next-ID allocator, treatment history/status required for audit, medical ledger, work counters, and canonical due/completion state in one canonical order.
- Restore must reconstruct and validate every derived index. Reject duplicate/noncanonical wound or treatment IDs, wrong ownership, stale generations, impossible timestamps/ranges, inconsistent health/life/casualty state, dead active participants, non-medics, cross-faction/cell active relationships, conflicting busy endpoints, invalid treatment target/kind, already-due completions, impossible recovered/bleeding combinations, invalid due coverage, ledger mismatch/overflow, arithmetic hazards, and trailing bytes.
- Snapshot/resume and uninterrupted execution must produce identical future IDs, events, state, treatment selection, and digest within the same fidelity path.
- Extend the server's strict command parser and complete event JSON serialization for every M1.2 command, event, enum, error, and death/interruption reason. Unknown fields, invalid combinations, unsupported versions, and out-of-range values fail before mutation.
- Update README; add ADR 0003 documenting the casualty model, treatment rules, event ordering, cold analysis, fidelity boundaries, conservation, and honest limitations; add `docs/m1-2-acceptance-map.md` mapping every required cluster below to concrete independent tests.

## Required tests

Preserve every M0 and M1.1 test and add adversarial coverage including:

1. One-second oracle versus analytical cold advancement across a literal deterministic matrix of no wound/one wound/multiple wounds, bleeding rates, shock and blood thresholds, health values, activities, treatment/control states, recovery states, durations, boundary-1/boundary/boundary+1, death precedence, and large-time cases. Assert complete independent expected state and events.
2. Wound command validation, stable monotonic IDs, multiple distinct wounds, immediate trauma, forced impairment/activity, exact rescheduling, and atomic rejection for dead/stale/invalid/overflowing targets.
3. Hot and cold casualty execution each match independent hand-authored fixtures at bleeding, shock, incapacitation, treatment, recovery, and death boundaries; cross-path equality alone is supplemental evidence.
4. Direct treatment start validates role, faction, cell, activity, impairment, busy endpoints, wound target, kind, inventory, and self-treatment atomically.
5. Request-treatment selection chooses the lowest eligible stable ID from the exact indexed faction/cell cohort; churn eligibility, inventory, death, removal, busy state, and generation reuse so stale or ineligible medics are never selected. Prove candidate visits are independent of unrelated world population.
6. Hemostatic and shock treatments consume the documented inventory once, take their exact full duration, provide no early benefit, complete once, change real target state, release both endpoints, and reschedule recovery.
7. Explicit interruption plus automatic interruption from medic death, patient death, immediate incapacitation, removal, and other documented eligibility loss emits one exact reason, never applies the completion, never refunds supplies, and leaves neither endpoint busy.
8. Multiple simultaneous casualties/treatments at the same timestamp obey global `EntityId` then `TreatmentId` ordering. A same-timestamp bleed/shock death defeats treatment completion, and scheduled commands execute afterward in schedule-ID order.
9. Medical source/carried/consumed/lost accounting remains equal across success, failed start, interruption, completion, death, removal, snapshot restore, and ledger-overflow rollback. Preserve food/water equations simultaneously.
10. Repeated wound/treatment/due churn through removal and generational ID reuse cannot act on a stale soldier, wound, medic, patient, or treatment ID.
11. Repeated hot↔cold cycles before/during/at/after treatment and through recovery/death preserve exact state, remaining duration, events, inventory, IDs, and indexes without double advancement.
12. Snapshot corruption covers every new encoded class and relationship with exact rejection categories; valid restored continuation is byte/event/digest deterministic and arbitrary query permutations remain pure.
13. Full current-segment rollback covers failures after an earlier internal bleed, recovery, interruption, or completion boundary. A later failing scheduled command preserves the exact earlier committed prefix and queued suffix.
14. A huge cold advance with no/few medical boundaries performs work proportional to actual due casualties/treatments, proven with deterministic structural counters rather than elapsed-time assertions. No population-wide or per-second cold scan is allowed.
15. Server tests pin every new command, event, error, enum, reason, strict malformed-input rejection, and unchanged digest after invalid wire input.

Avoid tests that derive expected values by invoking the implementation under test or merely compare two executions of the same potentially broken transition. Use literal fixtures, independent arithmetic, conservation equations, exact event arrays, and private structural index/counter assertions.

## Performance proof

Extend the benchmark without deleting valid M0/M1.1 evidence.

### Bounded CI workload

Use environment controls so CI executes at least:

- 10,000 individually stored soldiers with varied living and medical states;
- multiple distinct wounds across a representative cold cohort, including active bleeding, shock, stabilization, recovery, and death boundaries;
- enough medics and simultaneous treatment requests to exercise deterministic indexed selection, timed completion, interruption, and medical consumption;
- a bounded hot-cell cohort performing real one-second living and casualty transitions;
- full individual living/casualty/wound/inventory checksum work.

Report separate counts/times for cold advance, living boundaries, medical boundaries, selection candidates visited, treatments started/completed/interrupted, recovery transitions, hot member-steps, full checksum, snapshot, digest, current RSS, and peak RSS.

### Full manual workload

Run 2,410,000 individually stored soldiers for at least 60 simulated seconds with varied non-default living state and a deterministic full per-soldier casualty/wound checksum. Include a separately reported representative wounded cohort, medic-selection cohort, active treatment cohort, recovery cohort, and hot cohort; do not make all 2,410,000 soldiers inert defaults.

Report simulated-seconds-per-wall-second only for clearly named measured phases. Retain the named CPU/RAM/OS/Rust environment and the one-simulated-second-per-wall-second design target. If the target fails, report the measured failure honestly and profile before proposing optimization. Do not call this combat, battlefield rendering, complete logistics, or complete M1.

## Verification and handoff

Run:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- repeat the new parity/selection/interruption/snapshot tests enough to expose nondeterminism
- `cargo build --workspace --release`
- bounded CI benchmark
- full 2,410,000-soldier benchmark
- `git diff --check`
- clean `git status --short --branch`

Produce one clean local implementation commit on top of the exact PR head supplied by the owner. Report the local commit SHA, exact test and benchmark output, limitations, and explicit owner-upload instruction. Do not claim GitHub changed, resolve review threads, mark ready, merge, begin M1.3, or silently weaken acceptance criteria.
