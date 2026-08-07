# ADR 0001: Authoritative state and simulation level of detail

## Status

Accepted.

## Decision

`sim-core::World` is the only authoritative owner. Every soldier occupies one slot in dense, structure-of-arrays storage and is addressed by a slot plus generation. Despawning invalidates every old handle before the slot can be reused. Snapshots, the server, WASM, and future renderers receive serialized values or commands; none owns a second soldier.

Cold soldiers store need values and the time at which those values were materialized. Their current needs are an integer function of elapsed simulation seconds, so advancing time does not scan them. A deterministic ordered scheduler processes only due world events. Hot-cell membership is authoritative metadata for a future fixed-step tactical system; changing fidelity does not move, clone, or delete entity records.

Commands are applied serially against a monotonic clock. Ordered maps/sets, fixed-width integer quantities, a seeded counter-based random stream, and versioned little-endian snapshots (including pending commands) prevent dependence on hash iteration, wall time, thread scheduling, or local entropy.

## Consequences

Queries pay the small cost of materializing one soldier's needs. Global digests and snapshots intentionally scan live storage. The first snapshot format favors auditability over compression. Tactical combat, persistence migration, authenticated networking, and parallel execution require later ADRs without weakening single ownership.

## M0 boundary and correctness details

M0 implements deterministic kernel mechanisms, not M1 living-world behavior. Only `WorldCommand` values can be scheduled; external `Command::AdvanceTo` is excluded from the queue at the type level. Same-time work is consumed by index in linear time. A failed event remains queued with later work at that timestamp in exact order and leaves the clock at the failure time. Snapshot version 3 serializes free-list order, uses tagged lossless options, and validates allocation, schedule, vector, squad, officer, member uniqueness, and soldier back-reference invariants.

Needs use `u64` elapsed seconds and saturate each public `u32` need at `u32::MAX`; therefore every `u64` timestamp, including `u64::MAX`, is supported without truncation or an iterative wakeup loop.

Squad detach uses the soldier's validated authoritative back-reference rather than scanning all squads. Generation-exhausted slots retire rather than wrap, and RNG counter exhaustion has explicit modulo-2^64 behavior. Resource totals enter the ledger only through typed stockpile creation events; transfers conserve totals, and arbitrary mutation is not exposed.
