# M1.1 living-state benchmark

Measured on 2026-08-07 in the configured cloud container. Both runs store one authoritative record per soldier, use varied integer activity and inventory state, execute real cold due transitions and a bounded indexed hot cohort, and checksum every projected living field, life/death detail, activity, materialization timestamp, health projection, and carried-inventory field. They do not measure combat or complete M1.

## Full manual run

```text
soldiers=2410000
initialization_seconds=12.285253
dense_scheduler_commands=100000
advance_simulated_seconds=63
cold_advance_seconds=0.109489
mixed_hot_due_advance_seconds=11.594168
repeated_one_second_advances=3
repeated_one_second_seconds=0.037367
repeated_one_second_events=3
advance_events=2510008
cold_due_transitions=2409000
hot_indexed_members=1000
hot_member_steps=43000
full_living_checksum_seconds=2.091433
simulated_seconds_per_wall_second=4.555
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=e65a191484e431c5
snapshot_seconds=5.092830
snapshot_bytes=190772252
digest_seconds=5.370663
digest=61a9af5c944b724e
current_rss_kib=761920
peak_rss_kib=948032
```

The throughput denominator is the named cold, mixed hot/due, repeated one-second, and full checksum phases. Initialization, snapshot, and digest are reported separately. The 4.555 simulated-seconds-per-wall-second result exceeds the one-to-one living-state target in this environment. Three repeated one-second calls over 2,410,000 records took 0.037367 seconds total because only the indexed hot cohort was touched; advances and fidelity changes use touched-record staging and do not clone the world. Peak RSS (948,032 KiB) includes the canonical snapshot buffer, not a second `World`.

## Bounded run

```text
soldiers=10000
initialization_seconds=0.025268
dense_scheduler_commands=1000
advance_simulated_seconds=63
cold_advance_seconds=0.000386
mixed_hot_due_advance_seconds=0.054372
repeated_one_second_advances=3
repeated_one_second_seconds=0.000153
repeated_one_second_events=3
advance_events=11008
cold_due_transitions=9900
hot_indexed_members=100
hot_member_steps=4300
full_living_checksum_seconds=0.006902
simulated_seconds_per_wall_second=1019.213
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=4da48dbe9bd1161d
snapshot_seconds=0.007685
snapshot_bytes=807452
digest_seconds=0.009188
digest=083bcf6c43240d0d
current_rss_kib=5904
peak_rss_kib=6528
```

## Environment and limitations

The run used the configured Linux x86-64 cloud container and Rust release profile. RSS values come from `/proc/self/status`. Results cover deterministic living physiology, scheduling, fidelity membership, persistence, and digest work only. They exclude combat, AI, pathfinding, graphics, durable networking, vehicles, and the remainder of M1.
