# Persistent Armageddon deterministic kernel (M0)

This Rust workspace implements **M0 kernel behavior**: one authoritative record per soldier ID, a monotonic clock, deterministic schedulable world commands, conserved stock transfers, integer derived needs, relationship integrity, and versioned validated snapshots. It is not a living-world or combat simulation.

## Architecture

- `sim-core`: generational IDs, structure-of-arrays records, squads/officers, logistics ledger, ordered scheduler, snapshots/replay digests, and lazy needs.
- `sim-server`: minimal `GET /health` and `GET /snapshot` boundary plus the manual benchmark.
- `sim-wasm`: dependency-free adapter suitable for the optional WASM target.

Only `Role::Officer` soldiers may be assigned as officers. A soldier has at most one squad; reassignment removes the old squad's membership and officer pointer. Scheduled commands exclude time advancement at the Rust type level. When scheduled work fails, the clock remains at that event time and the failing and unattempted commands remain pending for explicit operator correction/recovery.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo run --release -p sim-server -- --benchmark
```

`PA_SOLDIERS` bounds the benchmark for CI; its manual default is 2,410,000. See [benchmark evidence](docs/benchmark.md) and [ADR 0001](docs/adr/0001-authoritative-state-and-lod.md).

## Not implemented (M1 and later)

Combat, tactical fixed steps, living-world behavior, graphics, pathfinding, lore systems, authenticated networking, and persistence migrations are not implemented. Hot cells are metadata scaffolding only and must not be presented as a combat/LOD implementation.
