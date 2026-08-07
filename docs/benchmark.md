# M0 execution-skeleton benchmark

## Full manual run

Measured on 2026-08-07 with:

```text
$ time cargo run --release -p sim-server -- --benchmark
soldiers=2410000
initialization_seconds=0.675641
dense_scheduler_commands=100000
advance_simulated_seconds=60
advance_events=100001
hot_cell_steps_included=true
full_needs_pass_included=true
combined_advance_wall_seconds=0.213009
simulated_seconds_per_wall_second=281.678
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
needs_checksum=3399153494c8c265
snapshot_seconds=0.621268
snapshot_bytes=118480100
digest_seconds=1.003303
digest=73af73890470b3e9
current_rss_kib=265916
peak_rss_kib=381408

real    0m2.662s
user    0m1.489s
sys     0m1.169s
```

Initialization, snapshot, and digest are separately timed and excluded from advance throughput. The defined advance workload combines ordered execution of 100,000 same-time hot-cell activations and one transfer across 60 simulated seconds with the complete derived-needs pass over all 2,410,000 live records. Hot-cell fixed-step counters are advanced across clock segments. Its measured 281.678 simulated-seconds per wall-second exceeds the design goal of 1, but this is evidence only for the M0 skeleton.

It does **not** measure or predict full warfare, combat, AI, pathfinding, graphics, networking, or M1 needs consumption. The hot work updates exact per-cell step counters; it does not scan soldiers or execute combat.

## Bounded CI run

```text
$ PA_SOLDIERS=10000 PA_DENSE_COMMANDS=1000 cargo run --release -p sim-server -- --benchmark
soldiers=10000
initialization_seconds=0.003215
dense_scheduler_commands=1000
advance_simulated_seconds=60
advance_events=1001
hot_cell_steps_included=true
full_needs_pass_included=true
combined_advance_wall_seconds=0.000929
simulated_seconds_per_wall_second=64602.961
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
needs_checksum=c6a9060f4fc58285
snapshot_seconds=0.003551
snapshot_bytes=508100
digest_seconds=0.003890
digest=3c84dc6afe1df948
current_rss_kib=3620
peak_rss_kib=4104
```

Timings use `Instant`; RSS and peak RSS use Linux `/proc/self/status`. Unit tests assert deterministic state and event order, never timing.
