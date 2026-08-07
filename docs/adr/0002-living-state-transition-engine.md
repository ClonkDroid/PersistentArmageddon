# ADR 0002: deterministic living-state transitions

## Status

Accepted for M1.1.

## Rules

Authoritative needs use integer units capped at 1,000. Each second, rest changes fatigue/sleep debt by -2/-2 and hunger/thirst by +1/+1; idle changes them by +1/+1 and +1/+2; march changes them by +3/+2 and +2/+3. At 100 hunger or thirst, one available ration is consumed and 100 need units are removed. March is forced to idle at 900 fatigue.

With no matching ration, hunger or thirst at 800 is severe. Each severe transition removes one morale point and either four health (starvation) or ten health (dehydration, which takes precedence). Health zero produces exactly one persistent death event; dead entities remain addressable but never transition again. Explicit activity changes reject dead or stale IDs atomically.

## Execution and ordering

One pure one-second transition result defines semantics and contains the complete next living state, inventory, accounting deltas, and typed events; both capacity staging and commit consume that result. A cold entity has one reverse-indexed due time. The cold path uses integer rate multiplication between consumption, threshold, forced-activity, deterioration, and death boundaries; unrelated command timestamps do not visit cold records. Queries project a private copy to the world clock, so they remain exact and cannot change snapshots, digests, indexes, or counters. Due entities and hot members at the same timestamp emit automatic work in one stable entity-ID order. Hot cells use a cell-membership index and apply the same transition once per elapsed second to actual living members. Fidelity changes materialize the boundary, remove or install the due entry, and never advance a member twice.

For a timestamp shared with scheduled commands, due cold entities and indexed living hot members are gathered, deduplicated, and processed in stable entity-ID order. Scheduled commands then execute by stable schedule ID. Advances retain M0 committed-prefix semantics: completed living work, the clock and `TimeAdvanced`, and successful lower schedule IDs remain committed and are returned on failure; the failing command and exact higher-ID suffix stay queued. One automatic segment, from the current committed clock to the next scheduled timestamp or requested target, is protected by a touched-record journal spanning every internal boundary. An error restores its exact soldiers, inventories, due entries, hot timestamps, ledgers, counters, and events; earlier scheduled segments remain the valid committed prefix. Journal storage is proportional to due records and indexed hot members reached by that segment rather than total world size. Fidelity changes stage only that cell's members and replacement due times. No `World` clone is used by advances, zero-delta calls, or fidelity commands.

## Persistence and accounting

Snapshot v6 records every living field, death time/cause, materialization time, food/water ledger, and canonical due index. Restore recomputes the exact next boundary and rejects altered, stale, duplicate, hot, dead, or impossible entries, conflicting health projections, bad ranges, future/death timestamps, noncanonical order, checked-ledger failures, and trailing bytes. Sources equal carried plus consumed plus explicit losses. `u64::MAX` is a closed terminal instant: a zero-delta advance is a no-op and an alive cold entity at that instant has no representable future due entry.

## Limitations

M1.1 contains no combat, wounds, treatment, vehicles, routes, higher formations, communications, AI, graphics, or setting assets. Hot stepping is deliberately proportional to elapsed seconds times indexed members. The analytical cold path is sparse only between discrete consequences; prolonged deprivation legitimately creates one deterioration boundary per second until death.
