# M0 release benchmark — Codex cloud environment

## Full manual run

Measured on 2026-08-07 from this change's working tree. The full command and output were:

```text
$ time cargo run --release -p sim-server -- --benchmark
soldiers=2410000
initialization_seconds=0.855468
sparse_scheduler_advance_seconds=0.000005
needs_full_pass_seconds=0.043325
needs_checksum=d3e48bb593d02e40
snapshot_seconds=1.003688
snapshot_bytes=161470100
digest_seconds=1.253976
digest=d3f107a8f849764a
current_rss_kib=327228
peak_rss_kib=484760

real    0m3.241s
user    0m1.394s
sys     0m1.844s
```

Initialization creates 2,410,000 individually addressable authoritative records. The sparse scheduler measurement advances to second 60 and processes one transfer; it is **not** described as million-soldier simulation throughput. The separately timed complete needs pass visits every live record and materializes/checks derived needs into the reported deterministic checksum. Snapshot creation and digest computation each scan authoritative state. Current RSS (`VmRSS`) and peak RSS (`VmHWM`) come from `/proc/self/status` after these phases; allocations freed or retained by the allocator and transient snapshot/digest buffers affect them.

## Reproduction and CI

```sh
cargo run --release -p sim-server -- --benchmark
PA_SOLDIERS=10000 cargo run --release -p sim-server -- --benchmark
```

The first command is the manual full benchmark. CI uses the second bounded smoke workload. `PA_SOLDIERS` changes record count, so checksums, snapshot size, memory, and timings differ.

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
