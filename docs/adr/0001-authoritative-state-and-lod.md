# ADR 0001: Authoritative state and deterministic execution skeleton

## Status

Accepted.

## Decision

`World` owns the sole entity record per generational ID, and `World::apply` is its only public mutation surface. Commands are typed; accepted mutations produce timestamped events. Advance returns committed-prefix events and a terminal error together, retaining the failing command and exact unattempted suffix. Stable scheduled IDs provide observable cancellation, while intrinsic invalidity is rejected at scheduling time.

Spawn and removal are explicitly modeled as scenario-loadout source and casualty/removal loss events containing exact carried quantities. Transfers are atomic and conserved. M0 does not implement consumption or production.

Each hot cell stores activation time, last stepped time, and an exact one-second fixed-step count. Every clock segment advances active cells before commands at its endpoint, so activation at T affects only later intervals and deactivation observes all work through T. Cold cells receive no per-step work. Same-time command insertion order determines transitions. This is only an execution skeleton; it contains no combat.

Snapshots use canonical little-endian version 4 encoding. They include allocator order and retirement, RNG and schedule ID counters, pending order, relationships, loadouts, ledger state, and hot-cell execution state. Restore rejects invalid tags, references, times, counters, duplicates, and trailing bytes. Digests cover the snapshot.

## Consequences

Integer state, ordered collections, monotonic time, and fixed ordering make snapshot continuation match uninterrupted execution. Hot work is O(active cells × clock segments), not an all-soldier scan. Server and benchmarks submit commands through the same boundary. Combat, full living needs, graphics, lore, prediction, and authenticated multiplayer remain outside M0.
