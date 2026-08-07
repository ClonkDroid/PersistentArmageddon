# ADR 0001: Authoritative state and deterministic execution skeleton

## Status

Accepted.

## Decision

`World` owns the sole entity record per generational ID, and `World::apply` is its only public mutation surface. Commands are typed; accepted mutations produce timestamped events. Advance returns committed-prefix events and a terminal error together, retaining the failing command and exact unattempted suffix. Stable scheduled IDs have an authoritative ID-to-time index for logarithmic cancellation, while intrinsic invalidity and conflicting reserved stockpile creation are rejected at scheduling time.

Spawn and removal are explicitly modeled as scenario-loadout source and casualty/removal loss events containing exact carried quantities. Transfers are atomic and conserved. M0 does not implement consumption or production.

Each hot cell stores activation time, last stepped time, and an exact one-second fixed-step count. Every clock segment advances active cells before commands at its endpoint, so activation at T affects only later intervals and deactivation observes all work through T. Its event reports `hot_cells_stepped` and `fixed_steps_per_hot_cell`; both are zero for a cold segment, and their aggregate can be computed in `u128`. Cold cells receive no per-step work. Same-time command insertion order determines transitions. This is only an execution skeleton; it contains no combat.

Snapshots use canonical little-endian version 5 encoding. They include allocator order and retirement, RNG and schedule ID counters, pending order, relationships, loadouts, ledger state, and hot-cell execution state. Restore reconstructs and validates cancellation and reservation indexes, rejects noncanonical ordering, and requires the exact active-window step equation. Digests cover the snapshot.

## Consequences

Integer state, ordered collections, monotonic time, and fixed ordering make snapshot continuation match uninterrupted execution. Hot work is O(active cells × clock segments), not an all-soldier scan. Server and benchmarks submit commands through the same boundary. Combat, full living needs, graphics, lore, prediction, and authenticated multiplayer remain outside M0.
