# Persistent Armageddon deterministic kernel (M1.1)

This workspace implements the M0 execution guarantees plus the M1.1 living-state increment. It is not a combat simulation or complete M1.

## Authoritative boundary

`sim-core::World::apply` is the sole public mutation boundary. Every soldier owns integer hunger, thirst, fatigue, sleep debt, morale, health, activity, life/death state, carried rations, and a materialization timestamp. Idle, rest, and march use different documented rates; ration consumption removes inventory and updates the conservation ledger; deprivation emits deterioration and one persistent death. Queries are read-only.

Cold soldiers are indexed by their next discrete consequence and analytically materialized between consequences. Hot cells step only their indexed members, in stable entity-ID order, with the same one-second transition. At a shared timestamp, living work completes first, then scheduled commands execute in schedule-ID order. Hot cells retain fixed-step telemetry, but the real work is reported separately. See [ADR 0002](docs/adr/0002-living-state-transition-engine.md).

Soldier spawn is an explicit scenario-loadout source and removal is an explicit loss. Food and water track sourced, carried, consumed, and explicitly lost totals. Ammunition, medical inventory, squads, stockpiles, deterministic scheduling/cancellation, monotonic time, and deterministic RNG retain M0 behavior.

## Server wire format

The loopback-only server accepts `POST /v1/command`, including `set_activity` with a raw entity `id` and `activity` of `rest`, `idle`, or `march`. Responses serialize all automatic living events with exact timestamps and before/after audit data. Malformed or unsupported values fail before mutation. `GET /health` and `GET /snapshot` remain available. This is not a durable or multi-writer production transport; clients must not automatically retry ambiguously delivered mutations.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
PA_SOLDIERS=10000 PA_DENSE_COMMANDS=1000 cargo run --release -p sim-server -- --benchmark
cargo run --release -p sim-server -- --benchmark
```

Snapshot version 6 canonically includes living state, food/water accounting, due transitions, and work counters; cell membership and reverse indexes are reconstructed and validated strictly. Version 5 is deliberately rejected. The manual benchmark defaults to 2,410,000 records and 100,000 dense commands. This increment does not implement combat, wounds, treatment, vehicles, routes, higher formations, communications, AI, graphics, or setting assets.
