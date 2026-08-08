# M1.2 contract-to-test map

**Gate B acceptance evidence is complete.** Acceptance tranche 1 implements recovery scenarios 01–05, tranche 2A implements scenarios 06–08, tranche 2B implements scenarios 09–10, tranche 3A implements scenarios 11–12, and tranche 3B implements scenarios 13–14. Gate C, medical benchmarks, production review, overall M1.2 completion, and M1.3 remain out of scope and are not claimed.

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
| 09 real-entity availability churn and exact candidate visits | Implemented in tranche 2B | — | `gate_b_acceptance_09_real_medic_churn_visits_only_eligible_candidates` |
| 10 medic/patient removal, conservation, and generation cleanup | Implemented in tranche 2B | — | `gate_b_acceptance_10_materialized_removal_cleans_both_endpoint_roles` |
| 11 elapsed hot/cold treatment and recovery cycles | Corrected after Gate B audit | — | `gate_b_acceptance_11_repeated_fidelity_cycles_preserve_medical_deadlines` (nonzero hot/cold intervals, literal fidelity/advance arrays, exact private due/hot state) |
| 12 rollback after recovery/healing/availability/interruption work | Corrected after Gate B audit | — | `gate_b_acceptance_12_late_recovery_overflow_rolls_back_all_earlier_medical_work` (literal control commit and full failed-segment rollback) |
| 13 combined active-treatment/recovery snapshot continuation and corruption rejection | Implemented in tranche 3B | — | `gate_b_acceptance_13_combined_snapshot_continuation_and_recovery_rejection` (one v7 image, literal future events, byte/digest/index parity, four exact recovery-state rejections) |
| 14 bleeding/recovery query purity including private authority | Implemented in tranche 3B | — | `gate_b_acceptance_14_bleeding_and_recovery_queries_are_fully_pure` (repeated permutations, literal projections, unchanged authority/indexes/counters) |

Historical tests in `sim-core/tests/medical.rs` remain supplemental regression coverage only; they are not substitutes for the dedicated acceptance scenarios above.
