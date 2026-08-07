# Persistent Armageddon deterministic kernel (M0)

This workspace implements the M0 deterministic execution skeleton, not combat or M1 living-world behavior.

## Authoritative boundary

`sim-core::World::apply` is the sole public mutation boundary. Typed commands cover soldier source/loss lifecycle, squads and officers, stockpiles and transfers, fidelity, scheduling/cancellation, time, and RNG. Every accepted mutation emits a timestamped event. An advance outcome retains committed-prefix events together with a terminal error; the failing scheduled command and exact suffix remain queued at the failure timestamp. Intrinsically invalid scheduled commands are rejected immediately, while stable schedule IDs allow state-dependent failures to be cancelled.

Soldier spawn is an explicit scenario-loadout source and removal is an explicit loadout loss. Their events identify the soldier and exact ammunition, food, water, and medical quantities. These are M0 scenario/casualty accounting events, not production or consumption.

Hot cells have a deterministic one-second fixed-step counter. Clock segments advance only active cells; cold cells remain sparse. `TimeAdvanced` reports the exact active-cell count and fixed steps per active cell for each segment, avoiding an overflowing aggregate product. This is a queryable execution skeleton and does not claim tactical combat.

## Server wire format

The server is an M0 **loopback-only, single-authoritative-writer boundary**, not a production multi-client transport. It retains `GET /health` and `GET /snapshot`, and accepts `POST /v1/command` with `Content-Length` and JSON no larger than 64 KiB. The body has `version: 1`, a `command` (`advance_to`, `create_stockpile`, `transfer`, `set_region_hot`, `schedule_hot`, `schedule_transfer`, or `cancel_scheduled`), the command's named fields, and `null` for the remaining optional fields. Unsupported, malformed, oversized, or length-mismatched requests are rejected before mutation. Responses contain version, clock, ordered timestamped events, nullable terminal error, and resulting digest.

Commands commit before their HTTP response is written. A POST may therefore commit even when its response is lost, and a client **must never automatically retry an ambiguously delivered mutation**. Recovery is: stop all command submission; fetch `/health` and the authoritative `/snapshot`; reconcile the intended command against the complete state, pending queue, clock, and digest; and require operator recovery when the result cannot be proved. The digest detects state equality but does not identify which command produced that state. Durable client request IDs, a persistent idempotent command receipt/log, restart persistence, and multi-writer conflict control are mandatory before public or multi-client deployment and are intentionally outside M0.

Each accepted connection has one absolute deadline covering header and body reads plus response-header and response-body writes, including incremental snapshot delivery. A timed-out or otherwise failed peer is closed without terminating the authoritative listener.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
PA_SOLDIERS=10000 PA_DENSE_COMMANDS=1000 cargo run --release -p sim-server -- --benchmark
cargo run --release -p sim-server -- --benchmark
```

Snapshot version 5 includes schedule IDs, canonical pending schedule data from which cancellation and stockpile-reservation indexes are reconstructed, and hot-cell execution state and is decoded strictly. The manual benchmark defaults to 2,410,000 records and 100,000 dense commands. See [benchmark evidence](docs/benchmark.md) and [ADR 0001](docs/adr/0001-authoritative-state-and-lod.md).
