# M1.1 living-state benchmark

Measured on 2026-08-07 in the configured cloud container. Both runs store one authoritative record per soldier, use varied integer activity and inventory state, execute real cold due transitions and a bounded indexed hot cohort, and checksum every projected living field, life/death detail, activity, materialization timestamp, health projection, and carried-inventory field. They do not measure combat or complete M1.

## Full manual run

```text
soldiers=2410000
initialization_seconds=5.363960
dense_scheduler_commands=100000
advance_simulated_seconds=63
cold_advance_seconds=0.083586
mixed_hot_due_advance_seconds=10.389577
repeated_one_second_advances=3
repeated_one_second_seconds=0.039170
repeated_one_second_events=3
advance_events=2510008
cold_due_transitions=2409000
hot_indexed_members=1000
hot_member_steps=43000
full_living_checksum_seconds=0.723379
simulated_seconds_per_wall_second=5.607
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=e65a191484e431c5
snapshot_seconds=0.877676
snapshot_bytes=190772252
digest_seconds=1.359533
digest=61a9af5c944b724e
current_rss_kib=1082476
peak_rss_kib=1322764
```

The throughput denominator is the named cold, mixed hot/due, repeated one-second, and full checksum phases. Initialization, snapshot, and digest are reported separately. The 5.607 simulated-seconds-per-wall-second result exceeds the one-to-one living-state target in this environment. Three repeated one-second calls over 2,410,000 records took 0.039170 seconds total because only the indexed hot cohort was touched; advances and fidelity changes use touched-record staging and do not clone the world. Peak RSS (1,322,764 KiB) includes the canonical snapshot buffer, not a second `World`.

## Bounded run

```text
soldiers=10000
initialization_seconds=0.014733
dense_scheduler_commands=1000
advance_simulated_seconds=63
cold_advance_seconds=0.000606
mixed_hot_due_advance_seconds=0.036758
repeated_one_second_advances=3
repeated_one_second_seconds=0.000261
repeated_one_second_events=3
advance_events=11008
cold_due_transitions=9900
hot_indexed_members=100
hot_member_steps=4300
full_living_checksum_seconds=0.002327
simulated_seconds_per_wall_second=1576.928
design_goal_simulated_seconds_per_wall_second=1.000
design_goal_met=true
living_checksum=4da48dbe9bd1161d
snapshot_seconds=0.002139
snapshot_bytes=807452
digest_seconds=0.004645
digest=083bcf6c43240d0d
current_rss_kib=8228
peak_rss_kib=8624
```

## Environment and limitations

The run used the configured Linux x86-64 cloud container and Rust release profile. RSS values come from `/proc/self/status`. Results cover deterministic living physiology, scheduling, fidelity membership, persistence, and digest work only. They exclude combat, AI, pathfinding, graphics, durable networking, vehicles, and the remainder of M1.

## M1.2 verification run (2026-08-08)

After the snapshot-v7 casualty state addition, the bounded run reported 10,000 soldiers, 0.048688 seconds for mixed hot/due advance, 1,197.700 simulated-seconds per wall-second, an 807,528-byte snapshot, digest `bb18096f2ade74a1`, and 9,328 KiB peak RSS. The full run reported 2,410,000 soldiers, 11.791486 seconds for mixed hot/due advance, 5.073 simulated-seconds per wall-second, a 190,772,328-byte snapshot, digest `587d42fff34e035c`, and 1,523,808 KiB peak RSS.

The existing benchmark still measures real living-state population work and snapshot/digest coverage. It does **not** yet create a representative wounded/medic/treatment cohort or report M1.2-specific boundary and selection counters, so these results are not evidence for the M1.2 casualty performance requirement and must not be described as such.
