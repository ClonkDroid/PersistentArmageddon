# ADR 0001: Authoritative state and simulation level of detail

## Status

Accepted.

## Decision

`sim-core::World` is the only authoritative owner. Every soldier occupies one slot in dense, structure-of-arrays storage and is addressed by a slot plus generation. Despawning invalidates every old handle before the slot can be reused. Snapshots, the server, WASM, and future renderers receive serialized values or commands; none owns a second soldier.

Cold soldiers store need values and the time at which those values were materialized. Their current needs are an integer function of elapsed simulation seconds, so advancing time does not scan them. A deterministic ordered scheduler processes only due world events. Hot-cell membership is authoritative metadata for a future fixed-step tactical system; changing fidelity does not move, clone, or delete entity records.

Commands are applied serially against a monotonic clock. Ordered maps/sets, fixed-width integer quantities, a seeded counter-based random stream, and versioned little-endian snapshots (including pending commands) prevent dependence on hash iteration, wall time, thread scheduling, or local entropy.

## Consequences

Queries pay the small cost of materializing one soldier's needs. Global digests and snapshots intentionally scan live storage. The first snapshot format favors auditability over compression. Tactical combat, persistence migration, authenticated networking, and parallel execution require later ADRs without weakening single ownership.
