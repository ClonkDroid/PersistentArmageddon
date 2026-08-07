# M0 execution-skeleton benchmark

## Full manual run

Measured on 2026-08-07 with:

```text
$ time cargo run --release -p sim-server -- --benchmark
soldiers=2410000
initialization_seconds=0.731864
dense_scheduler_commands=100000
advance_simulated_seconds=60
advance_events=100004
hot_cell_steps_included=true
full_needs_pass_included=true
combined_advance_wall_seconds=0.217314
simulated_seconds_per_wall_second=276.098
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
needs_checksum=3399153494c8c265
snapshot_seconds=0.704155
snapshot_bytes=118480100
digest_seconds=1.046919
digest=a89bf1efbf1f27bc
current_rss_kib=267976
peak_rss_kib=383552

real    2.889s
user    1.527s
sys     1.364s
```

Initialization, snapshot, and digest are separately timed and excluded from advance throughput. The defined advance workload combines ordered execution of 100,000 same-time hot-cell activations and one transfer across 60 simulated seconds with the complete derived-needs pass over all 2,410,000 live records. Hot-cell fixed-step counters are advanced across clock segments. Its measured 276.098 simulated-seconds per wall-second exceeds the design goal of 1, but this is evidence only for the M0 skeleton.

## Measurement environment

```text
OS/kernel: Linux 6.18.35, x86_64 GNU/Linux
CPU: 3 vCPUs, Intel Xeon Platinum 8370C @ 2.80 GHz
RAM: 17 GiB available to the container, no swap
rustc: 1.95.0 (59807616e 2026-04-14), x86_64-unknown-linux-gnu, LLVM 22.1.2
cargo: 1.95.0 (f2d3ce0bd 2026-03-21)
profile: Cargo release profile (optimized defaults; no repository override)
```

It does **not** measure or predict full warfare, combat, AI, pathfinding, graphics, networking, or M1 needs consumption. The hot work updates exact per-cell step counters; it does not scan soldiers or execute combat.

## Bounded CI run

```text
$ PA_SOLDIERS=10000 PA_DENSE_COMMANDS=1000 cargo run --release -p sim-server -- --benchmark
soldiers=10000
initialization_seconds=0.003444
dense_scheduler_commands=1000
advance_simulated_seconds=60
advance_events=1004
hot_cell_steps_included=true
full_needs_pass_included=true
combined_advance_wall_seconds=0.000848
simulated_seconds_per_wall_second=70788.108
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
needs_checksum=c6a9060f4fc58285
snapshot_seconds=0.004014
snapshot_bytes=508100
digest_seconds=0.003972
digest=8506f3af80594579
current_rss_kib=3796
peak_rss_kib=4124
```

Timings use `Instant`; RSS and peak RSS use Linux `/proc/self/status`. Unit tests assert deterministic state and event order, never timing.
