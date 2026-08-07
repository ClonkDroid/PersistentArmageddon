# Codex task M1.1: deterministic living-state transition engine

Tracking issue: #3  
Parent roadmap: #2  
M0 base commit: `ff9bd2dfeb91305cc90bd5dc5b05886b28cf97ce`

## Objective

Replace M0's counter-only living-state placeholder with the first real per-soldier behavioral simulation.

After this increment, every soldier must remain an individually addressable authoritative entity whose food, hydration, sleep/fatigue, activity, morale, health/life status, and carried food/water evolve deterministically. A cold soldier may skip uneventful seconds analytically, but cannot be inert: all discrete consequences due by world clock T must have executed at their exact deterministic times. A hot soldier must execute the same defined one-second living transition for the actual members of its cell.

This is M1.1, not a complete war. Do not add combat, wounds, medical treatment, vehicles, routes, formations above squads, communications, graphics, copied lore, or proprietary assets.

## Non-negotiable semantics

### Individual authority

- Preserve one authoritative record per stable `EntityId`; never replace cold soldiers with population aggregates.
- Physiological death changes life state but does not automatically despawn/recycle the entity. Dead soldiers remain queryable until an explicit removal command.
- Queries must not mutate state or change snapshots/digests.
- All authoritative quantities use integers/fixed point. Results cannot depend on wall time, thread scheduling, hash iteration, or machine entropy.

### Living state

Define and document compact public types equivalent to:

- `Activity`: at least `Rest`, `Idle`, and `March`.
- `LifeState`: alive and dead with exact death time and deterministic cause.
- Persistent physiological state containing hunger, thirst, fatigue, sleep debt, morale, health, activity, and a materialization timestamp.
- Explicit integer constants/rates/thresholds for accumulation, recovery, ration use, deterioration, forced activity changes, morale effects, health damage, and death.

You may choose exact names, units, and balanced constants, but the rules must be simple, documented, deterministic, and exercised at every boundary. Food and water must actually be removed from each soldier's carried inventory when consumed. Rest must actually reduce fatigue/sleep debt. Severe unmet needs must cause observable deterioration and eventually death. Dead soldiers cannot eat, drink, recover, march, or produce repeated death events.

Add an authoritative command for changing a living soldier's activity. Reject invalid/dead targets atomically and emit a complete typed event. Automatic consumption, forced activity changes, deterioration, morale/health changes, and death must emit typed events containing the affected `EntityId` and exact before/after or delta data needed for auditing.

### One-second reference and sparse cold advancement

Implement one canonical one-second living transition and a sparse/event-driven cold path with exactly equivalent defined results.

- The reference step is the semantic oracle.
- The cold path must not loop once per elapsed second for every cold soldier.
- Its work must be bounded by actual state-transition boundaries (ration consumption, threshold crossing, activity change, damage/death, etc.), not raw elapsed duration.
- Maintain a deterministic due-transition index (or a rigorously equivalent structure) so `AdvanceTo(T)` executes every world-affecting transition due by T without scanning all cold soldiers every second and without leaving overdue deaths or consumption latent until query.
- Maintain a reverse due-time entry per live soldier. Spawn, explicit removal, activity change, inventory-changing interaction, fidelity transition, and restore must update the index without stale/duplicate entries.
- At a timestamp shared by automatic living transitions and scheduled commands, preserve M0's segment semantics: complete living work through T first, ordered by stable `EntityId`, then execute scheduled commands at T in schedule-ID order. Document and test this rule.
- Very large cold time advances must terminate in work proportional to actual boundaries and must not overflow or silently wrap.

### Hot cells and cell membership

The current hot-cell counter is not sufficient.

- Maintain an authoritative cell-membership index kept consistent on spawn and explicit removal. Do not find hot members by scanning every soldier.
- Each elapsed hot-cell second executes the one-second reference living transition for each actual living member in stable `EntityId` order.
- Prevent double advancement: a soldier is advanced by either the hot stepped path or cold due-transition path for a segment, never both.
- On cold→hot and hot→cold boundaries, materialize exact state at the boundary and preserve all fields, inventory, life state, due scheduling, and identity.
- Retain the useful hot-cell counters/telemetry, but they are no longer the claimed work.
- A full tactical combat loop is explicitly outside this PR.

### Conservation and accounting

Food and water cannot disappear merely by assignment.

- Extend authoritative accounting with explicit sourced, carried, consumed, and removed/lost totals for food and water, or an equivalent ledger that proves:
  `sources = currently carried + consumed + explicit losses`.
- Spawn/scenario loadout remains an explicit source; explicit removal remains a loss.
- Automatic consumption increments consumed totals exactly once.
- Failed commands and failed/saturated transitions are atomic.
- Snapshot and digest cover the accounting state.
- Preserve all existing ammunition/supply invariants.

### Snapshot, replay, and protocol

- Bump the snapshot format from v5 to v6 (supporting old v5 files is not required in M1.1; reject unsupported versions clearly).
- Encode living state, life/death details, materialization timestamps, cell membership or its canonical reconstructable source, due-transition state, and accounting canonically.
- Restore must reconstruct/validate all derived indexes and reject stale due entries, impossible timestamps, dead entities scheduled to act, inconsistent cell membership, impossible values, noncanonical order, trailing bytes, and arithmetic hazards.
- Snapshot/resume and uninterrupted execution must produce identical future events, IDs, state, and digest within the same fidelity path.
- Extend the server's command parser and complete event JSON serialization for M1.1. Malformed or unsupported values must fail before mutation.
- Update README and add an ADR for living-state semantics, event ordering, cold analytical advancement, fidelity boundaries, and honest limitations.

## Required tests

Preserve every M0 test and add adversarial coverage including:

1. One-second reference versus analytical cold advancement across a deterministic matrix of initial states, activities, inventories, threshold-adjacent values, durations, and large-time cases.
2. Exact hot-stepped versus cold-analytical soldier fields and typed automatic events at transition boundaries. Compare each path to independent expected fixtures; do not derive expectations from the other world.
3. Repeated hot↔cold cycles and snapshot continuation never duplicate, lose, reset, double-advance, resurrect, or teleport an entity.
4. Automatic food/water consumption and explicit removal satisfy the ledger equation; no double consumption at same-time boundaries.
5. Inventory exhaustion leads to documented deterioration/death at exact timestamps; dead records remain queryable and emit death once.
6. Rest, idle, and march have materially different state transitions and invalid activity commands are atomic.
7. Due-index churn under spawn/removal/generational ID reuse cannot act on stale IDs.
8. Same-timestamp living-transition/scheduled-command order is exact and stable.
9. Snapshot corruption tests cover every new invariant and derived-index reconstruction; query order cannot change snapshot bytes or digest.
10. Same seed and commands plus restore produce byte-identical ordered event sequences and equal digests.
11. A huge cold advance with no or few boundaries completes without per-second iteration; prove this with deterministic work counters, not elapsed-time thresholds.
12. Cell membership stays exact through spawn/removal/restore and hot processing touches only indexed members.

Avoid tests that merely compare two executions of the same potentially broken implementation. Use independent expected values and conservation equations.

## Performance proof

Extend the benchmark without replacing valid M0 evidence.

### Bounded CI workload

Use environment controls so CI executes at least:

- 10,000 individually stored soldiers with varied activities and inventories;
- real analytical living-state materialization/checksum work;
- actual automatic consumption/due transitions for a representative cohort;
- a bounded hot-cell stepped workload over indexed members.

Report separate counts/times for cold advance, due transitions, full individual materialization/checksum, hot member-steps, snapshot, digest, current RSS, and peak RSS.

### Full manual workload

Run 2,410,000 individually stored soldiers for at least 60 simulated seconds with varied non-default living state and a deterministic full per-soldier living-state checksum. Include actual due-transition and hot-member work in separately reported representative cohorts. Report simulated-seconds-per-wall-second only for clearly named measured phases; do not describe this as combat or complete M1.

Retain the named CPU/RAM/OS/Rust environment. If the 1 simulated second per wall second goal fails, report the measured failure honestly and profile before proposing optimization.

## Verification and handoff

Run:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- repeat new parity/fidelity/snapshot tests enough to expose nondeterminism
- `cargo build --workspace --release`
- bounded CI benchmark
- full 2,410,000-soldier benchmark
- `git diff --check`
- clean `git status --short --branch`

Produce one clean local implementation commit on top of the exact PR head supplied by the owner. Report the local commit SHA, benchmark output, limitations, and explicit owner-upload instruction. Do not claim GitHub changed, resolve threads, mark ready, merge, begin M1.2, or silently weaken acceptance criteria.