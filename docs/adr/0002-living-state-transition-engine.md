# ADR 0002: deterministic living-state transitions

## Status

Accepted for M1.1.

## Rules

Authoritative needs use integer units capped at 1,000. Each second, rest changes fatigue/sleep debt by -2/-2 and hunger/thirst by +1/+1; idle changes them by +1/+1 and +1/+2; march changes them by +3/+2 and +2/+3. At 100 hunger or thirst, one available ration is consumed and 100 need units are removed. March is forced to idle at 900 fatigue.

With no matching ration, hunger or thirst at 800 is severe. Each severe transition removes one morale point and either four health (starvation) or ten health (dehydration, which takes precedence). Health zero produces exactly one persistent death event; dead entities remain addressable but never transition again. Explicit activity changes reject dead or stale IDs atomically.

## Execution and ordering

The one-second transition defines semantics. A cold entity has one reverse-indexed due time. The cold path uses integer rate multiplication between consumption, threshold, forced-activity, deterioration, and death boundaries; it never scans every soldier for every second. Due entities at the same timestamp are processed by stable ID. Hot cells use a cell-membership index and apply the same transition once per elapsed second to actual members in stable ID order. Fidelity changes materialize the boundary, remove or install the due entry, and never advance a member twice.

For a timestamp shared with scheduled commands, all living transitions through that timestamp complete first. Scheduled commands then execute by stable schedule ID. A failed scheduled command retains the committed living prefix and exact command suffix, preserving M0 monotonic-clock semantics.

## Persistence and accounting

Snapshot v6 records every living field, death time/cause, materialization time, food/water ledger, and canonical due index. Restore reconstructs cell membership and reverse indexes and rejects bad ranges, future timestamps, dead due entries, missing live cold entries, noncanonical order, conservation failures, and trailing bytes. Sources equal carried plus consumed plus explicit losses.

## Limitations

M1.1 contains no combat, wounds, treatment, vehicles, routes, higher formations, communications, AI, graphics, or setting assets. Hot stepping is deliberately proportional to elapsed seconds times indexed members. The analytical cold path is sparse only between discrete consequences; prolonged deprivation legitimately creates one deterioration boundary per second until death.
