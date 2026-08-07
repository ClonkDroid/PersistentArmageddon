# M1.1 living-state benchmark

Measured on 2026-08-07 in the configured cloud container. Both runs store one authoritative record per soldier, use varied integer activity and inventory state, execute real cold due transitions and a bounded indexed hot cohort, and checksum every projected living field, life/death detail, activity, materialization timestamp, health projection, and carried-inventory field. They do not measure combat or complete M1.

## Full manual run

```text
soldiers=2410000
initialization_seconds=3.331889
dense_scheduler_commands=100000
advance_simulated_seconds=60
cold_advance_seconds=2.164622
mixed_hot_due_advance_seconds=2.970003
advance_events=2510005
cold_due_transitions=2409000
hot_indexed_members=1000
hot_member_steps=40000
full_living_checksum_seconds=0.368974
simulated_seconds_per_wall_second=10.902
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=84e908bb9f911fa4
snapshot_seconds=0.613004
snapshot_bytes=190772252
digest_seconds=1.034139
digest=98f8ce8017f62ce5
current_rss_kib=842428
peak_rss_kib=1028468
```

The throughput denominator is the named cold, mixed hot/due, and full checksum phases. Initialization, snapshot, and digest are reported separately. The 10.902 simulated-seconds-per-wall-second result exceeds the one-to-one living-state target in this environment.

## Bounded run

```text
soldiers=10000
initialization_seconds=0.009467
dense_scheduler_commands=1000
advance_simulated_seconds=60
cold_advance_seconds=0.005790
mixed_hot_due_advance_seconds=0.007523
advance_events=11005
cold_due_transitions=9900
hot_indexed_members=100
hot_member_steps=4000
full_living_checksum_seconds=0.001953
simulated_seconds_per_wall_second=3930.438
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=4a51dd1fd789b440
snapshot_seconds=0.003593
snapshot_bytes=807452
digest_seconds=0.003628
digest=d943b97308c875fe
current_rss_kib=6640
peak_rss_kib=7332
```

## Environment and limitations

The run used the configured Linux x86-64 cloud container and Rust release profile. RSS values come from `/proc/self/status`. Results cover deterministic living physiology, scheduling, fidelity membership, persistence, and digest work only. They exclude combat, AI, pathfinding, graphics, durable networking, vehicles, and the remainder of M1.
