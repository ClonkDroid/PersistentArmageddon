# Release benchmark — Codex cloud environment

## Reproduction

Measured on 2026-08-07 from commit working tree with:

```text
$ time cargo run --release -p sim-server -- --benchmark
soldiers=2410000
init_seconds=0.865824
advance_seconds=0.000004
sim_seconds=60
throughput_sim_seconds_per_wall_second=17137960.58
approx_rss_kib=169536
digest=9aff427b12c7de21

real    0m2.578s
user    0m1.276s
sys     0m1.301s
```

This is the required full 2,410,000-soldier run, not an extrapolation. Initialization creates an individually addressable record for every soldier. The advance processes a scheduled ammunition/supply transfer at second 30 and advances lazy needs bookkeeping through second 60. Approximate resident memory is read from Linux `/proc/self/status` immediately before the digest. Digest generation occurs after the reported advance time.

For this implemented sparse workload, the measured simulation throughput exceeds the design goal of one simulated second per wall-clock second. It must not be interpreted as evidence that future hot-cell combat will meet that goal.

## Environment

```text
CPU: 3 vCPU, Intel Xeon Platinum 8370C @ 2.80 GHz (KVM)
RAM: 17 GiB available, no swap
OS architecture: x86_64
rustc: 1.95.0 (59807616e 2026-04-14), LLVM 22.1.2
cargo: 1.95.0 (f2d3ce0bd 2026-03-21)
profile: release (optimized)
```

The container did not include `/usr/bin/time`; the shell's `time` keyword supplied total process timings. The benchmark's own `Instant` measurements and Linux resident-memory reading were unaffected.

## Next measurements

When tactical fixed-step behavior is implemented, benchmark representative hot-cell populations separately, record percentile step latency, and profile digest/snapshot allocation independently from advancing the world.
