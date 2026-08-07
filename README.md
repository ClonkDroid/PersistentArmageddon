# Persistent Armageddon simulation foundation

This workspace implements deterministic, headless simulation truth for individually addressable soldiers. It is a foundation—not a claim of complete warfare fidelity.

## Architecture

- **`sim-core`** owns the monotonic `u64` clock, generational IDs, structure-of-arrays soldier data, explicit squads/officers, integer needs, deterministic random values, conserved stock transfers, ordered events, versioned snapshots, and replay digests.
- **`sim-server`** exposes `GET /health` and `GET /snapshot` on `127.0.0.1:8080`. Its `--benchmark` scenario creates persistent soldiers and runs scheduled logistics for 60 simulation seconds.
- **`sim-wasm`** is a dependency-free adapter that can compile for `wasm32-unknown-unknown` when the target is installed.

Cold soldiers are not scanned per frame or simulation second. Need rates are materialized from a stored base and timestamp only when queried. The timing queue visits due events in stable time/insertion order. Hot cells are marked for a future tactical fixed-step path while retaining the same authoritative records. See [ADR 0001](docs/adr/0001-authoritative-state-and-lod.md).

## Build and verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo run --release -p sim-server -- --benchmark
# Optional, after rustup target add wasm32-unknown-unknown:
cargo build -p sim-wasm --target wasm32-unknown-unknown
```

Set `PA_SOLDIERS` to run a smaller diagnostic scenario. The default and acceptance scenario is exactly 2,410,000.

## Benchmark evidence

Run details and measured output from the configured Codex environment are recorded in [`docs/benchmark.md`](docs/benchmark.md). “Throughput” measures this sparse 60-second scenario, not full tactical combat. The real-time design goal is only considered met for this implemented workload; richer combat requires fresh measurement.

## Known limitations

- Hot cells currently retain fidelity metadata but have no combat model.
- The HTTP boundary is deliberately minimal and single-threaded; authentication, command transport, persistence, and networking prediction are future work.
- Snapshot encoding is stable and auditable but not compressed.
