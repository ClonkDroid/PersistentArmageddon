# M0 release benchmark — Codex cloud environment

## Full manual run

Measured on 2026-08-07 from this change's working tree. The full command and output were:

```text
$ time cargo run --release -p sim-server -- --benchmark
soldiers=2410000
initialization_seconds=0.913441
dense_scheduler_commands=100000
dense_scheduler_seconds=0.017802
sparse_scheduler_advance_seconds=0.000003
needs_full_pass_seconds=0.172850
needs_checksum=3399153494c8c265
snapshot_seconds=0.975316
snapshot_bytes=154640092
digest_seconds=1.274194
digest=724c4c221796dc90
current_rss_kib=325760
peak_rss_kib=476504

real    0m3.455s
user    0m1.552s
sys     0m1.898s
```

Initialization creates 2,410,000 individually addressable authoritative records. The dense scheduler phase executes 100,000 `SetRegionHot` commands queued at one timestamp; the separately reported sparse phase advances to second 60 and processes one transfer. Neither is described as million-soldier simulation throughput. The separately timed complete needs pass visits every live record and hashes each ID and field's unambiguous bytes. Snapshot creation and digest computation each scan authoritative state. Current RSS (`VmRSS`) and peak RSS (`VmHWM`) come from `/proc/self/status` after these phases; allocations freed or retained by the allocator and transient snapshot/digest buffers affect them.

## Reproduction and CI

```sh
cargo run --release -p sim-server -- --benchmark
PA_SOLDIERS=10000 PA_DENSE_COMMANDS=1000 cargo run --release -p sim-server -- --benchmark
```

The first command is the manual full benchmark, including at least 100,000 dense commands. CI uses the second bounded smoke workload. The environment variables change work size, so checksums, snapshot size, memory, and timings differ. Unit tests assert state and order, never wall time.

## Environment and limitations

```text
CPU: 3 vCPU, Intel Xeon Platinum 8370C @ 2.80 GHz (KVM)
RAM: 17 GiB available, no swap
OS architecture: x86_64
rustc: 1.95.0 (59807616e 2026-04-14), LLVM 22.1.2
cargo: 1.95.0 (f2d3ce0bd 2026-03-21)
profile: release (optimized)
```

This measures only implemented M0 initialization, sparse scheduling, needs derivation, snapshots, and digests. It contains no combat, pathfinding, graphics, living-world M1 processing, persistence I/O, or network load. Future features require independent representative benchmarks rather than extrapolation from the sparse-clock time.
