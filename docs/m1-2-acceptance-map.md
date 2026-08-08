# M1.2 contract-to-test map

Gate B acceptance is **not complete or accepted**. Acceptance tranche 1 implements recovery scenarios 01–05, and tranche 2A implements scenarios 06–08. Scenarios 09–14 remain pending and no Gate C or M1.3 work is claimed.

| Gate B scenario | Status | Dedicated evidence | Private structural evidence |
| --- | --- | --- | --- |
| 01 recovery boundary −1/exact/+1 | Implemented in tranche 1 | `gate_b_acceptance_01_recovery_boundary_minus_exact_plus_one` | `gate_b_acceptance_private_01_canonical_recovery_due_is_exact` |
| 02 independent hot/cold large-leap recovery | Implemented in tranche 1 | `gate_b_acceptance_02_independent_hot_and_cold_large_leap_recovery` | `gate_b_acceptance_private_02_hot_and_cold_recovery_work_is_exact` |
| 03 two-wound final-control gating and ordered healing | Implemented in tranche 1 | `gate_b_acceptance_03_two_wound_final_control_gates_distinct_healing` | — |
| 04 successful shock care while a separate wound bleeds | Implemented in tranche 1 | `gate_b_acceptance_04_shock_care_while_bleeding_never_starts_recovery` | — |
| 05 nonfatal recovery cancellation and absolute-deadline restart | Implemented in tranche 1 | `gate_b_acceptance_05_new_wound_cancels_and_restarts_absolute_recovery` | — |
| 06 immediate wound and endpoint-interruption matrices | Implemented in tranche 2A | `gate_b_acceptance_06_immediate_wound_consequences_and_endpoint_interruptions` | — |
| 07 authoritative hot/cold command failure atomicity | Implemented in tranche 2A | — | `gate_b_acceptance_07_world_apply_failures_are_fully_atomic` |
| 08 automatic medic/patient interruption and death precedence | Implemented in tranche 2A | `gate_b_acceptance_08_automatic_incapacity_interrupts_before_completion` | `gate_b_acceptance_08_automatic_incapacity_preserves_audit_and_cleans_indexes`; `gate_b_acceptance_08_living_death_at_completion_has_role_precedence_and_cleans_indexes` |
| 09 real-entity availability churn and exact candidate visits | Pending | — | — |
| 10 medic/patient removal, conservation, and generation cleanup | Pending | — | — |
| 11 hot/cold cycles during treatment and recovery | Pending | — | — |
| 12 rollback after recovery/healing/availability/interruption work | Pending | — | — |
| 13 combined active-treatment/recovery snapshot continuation and corruption rejection | Pending | — | — |
| 14 bleeding/recovery query purity including private authority | Pending | — | — |

Historical tests in `sim-core/tests/medical.rs` remain supplemental regression coverage only; they are not substitutes for pending dedicated acceptance scenarios.
