# M1.2 contract-to-test map

| Cluster | Concrete evidence |
| --- | --- |
| Wound identity, combination, cold transition, v7 continuation | `distinct_wounds_bleed_and_snapshot_deterministically` |
| Indexed deterministic medic selection, conserved one-time consumption, exact duration and material completion | `lowest_eligible_medic_consumes_once_and_completion_controls_wound` |
| Explicit interruption, endpoint release, and no refund | `interruption_releases_endpoints_without_refund` |
| Existing M0/M1.1 invariants | `invariants`, `living_state`, and private invariant suites retained unchanged except v7/resource fixtures |

The benchmark remains a living-state population benchmark; it must not be represented as complete combat. M1.2 casualty cohort timings are reported separately when invoked by the server benchmark.
