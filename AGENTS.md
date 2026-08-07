# Repository invariants

- Maintain one authoritative entity record per stable ID.
- Simulation time never reverses, including on error paths and snapshot restore.
- Resources cannot appear or disappear without an explicit source or loss event.
- Restored execution must produce the same IDs and state as uninterrupted execution.
- Keep authoritative state integer or fixed-point and dependencies minimal.

## Code Review Rules

- Benchmarks must execute and time the work they claim to measure.
- Scaffolding and future intentions must not be described as implemented behavior.
- Require adversarial invariant tests for changes to scheduling, allocation, relationships, snapshots, or resource accounting.
