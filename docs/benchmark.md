# M1.1 living-state benchmark

Measured on 2026-08-07 in the configured cloud container. Both runs store one authoritative record per soldier, use varied integer activity and inventory state, execute real cold due transitions and a bounded indexed hot cohort, and checksum every projected living field, life/death detail, activity, materialization timestamp, health projection, and carried-inventory field. They do not measure combat or complete M1.

## Full manual run

```text
soldiers=2410000
initialization_seconds=4.339487
dense_scheduler_commands=100000
advance_simulated_seconds=63
cold_advance_seconds=0.052330
mixed_hot_due_advance_seconds=8.208261
repeated_one_second_advances=3
repeated_one_second_seconds=0.010306
repeated_one_second_events=3
advance_events=2510008
cold_due_transitions=2409000
hot_indexed_members=1000
hot_member_steps=43000
full_living_checksum_seconds=0.618439
simulated_seconds_per_wall_second=7.087
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=e65a191484e431c5
snapshot_seconds=0.945036
snapshot_bytes=190772252
digest_seconds=1.525054
digest=61a9af5c944b724e
current_rss_kib=1060184
peak_rss_kib=1246240
```

The throughput denominator is the named cold, mixed hot/due, repeated one-second, and full checksum phases. Initialization, snapshot, and digest are reported separately. The 7.087 simulated-seconds-per-wall-second result exceeds the one-to-one living-state target in this environment. Three repeated one-second calls over 2,410,000 records took 0.010306 seconds total because only the indexed hot cohort was touched; advances and fidelity changes use touched-record staging and do not clone the world. Peak RSS (1,246,240 KiB) includes the canonical snapshot buffer, not a second `World`.

## Bounded run

```text
soldiers=10000
initialization_seconds=0.013429
dense_scheduler_commands=1000
advance_simulated_seconds=63
cold_advance_seconds=0.000352
mixed_hot_due_advance_seconds=0.022912
repeated_one_second_advances=3
repeated_one_second_seconds=0.000165
repeated_one_second_events=3
advance_events=11008
cold_due_transitions=9900
hot_indexed_members=100
hot_member_steps=4300
full_living_checksum_seconds=0.003611
simulated_seconds_per_wall_second=2329.865
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=4da48dbe9bd1161d
snapshot_seconds=0.003690
snapshot_bytes=807452
digest_seconds=0.004658
digest=083bcf6c43240d0d
current_rss_kib=7820
peak_rss_kib=7820
```

## Environment and limitations

The run used the configured Linux x86-64 cloud container and Rust release profile. RSS values come from `/proc/self/status`. Results cover deterministic living physiology, scheduling, fidelity membership, persistence, and digest work only. They exclude combat, AI, pathfinding, graphics, durable networking, vehicles, and the remainder of M1.
