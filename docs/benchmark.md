# M1.1 living-state benchmark

Measured on 2026-08-07 in the configured cloud container. Both runs store one authoritative record per soldier, use varied integer activity and inventory state, execute real cold due transitions and a bounded indexed hot cohort, and checksum every living record. They do not measure combat or complete M1.

## Full manual run

```text
soldiers=2410000
initialization_seconds=9.650721
dense_scheduler_commands=100000
advance_simulated_seconds=60
cold_advance_seconds=1.404849
mixed_hot_due_advance_seconds=6.796631
advance_events=2510005
cold_due_transitions=2409000
hot_indexed_members=1000
hot_member_steps=40000
full_living_checksum_seconds=0.386521
simulated_seconds_per_wall_second=6.986
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=0bc99fd70403492d
snapshot_seconds=2.894698
snapshot_bytes=190772252
digest_seconds=3.497544
digest=ab786c73cb0b1505
current_rss_kib=733524
peak_rss_kib=919664
```

The throughput denominator is the named cold, mixed hot/due, and full checksum phases. Initialization, snapshot, and digest are reported separately. The 6.986 simulated-seconds-per-wall-second result exceeds the one-to-one living-state target in this environment.

## Bounded run

```text
soldiers=10000
initialization_seconds=0.039888
dense_scheduler_commands=1000
advance_simulated_seconds=60
cold_advance_seconds=0.002836
mixed_hot_due_advance_seconds=0.013777
advance_events=11005
cold_due_transitions=9900
hot_indexed_members=100
hot_member_steps=4000
full_living_checksum_seconds=0.000996
simulated_seconds_per_wall_second=3407.376
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=3dd0d5146d76e9fd
snapshot_seconds=0.005496
snapshot_bytes=807452
digest_seconds=0.007133
digest=4e7b32ad57f11d4e
current_rss_kib=5824
peak_rss_kib=6492
```

## Environment and limitations

The run used the configured Linux x86-64 cloud container and Rust release profile. RSS values come from `/proc/self/status`. Results cover deterministic living physiology, scheduling, fidelity membership, persistence, and digest work only. They exclude combat, AI, pathfinding, graphics, durable networking, vehicles, and the remainder of M1.
