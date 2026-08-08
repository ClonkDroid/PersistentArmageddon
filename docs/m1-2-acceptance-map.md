# M1.2 contract-to-test map

Each Gate B requirement has one newly added, dedicated acceptance fixture. Historical tests retain their descriptive names and are supplemental only.

| Gate B requirement | Dedicated fixture | Supplemental historical evidence |
| --- | --- | --- |
| Recovery boundary -1/exact/+1 and canonical due state | `gate_b_acceptance_01_recovery_boundary_minus_exact_plus_one` | `recovery_has_an_exact_material_boundary_and_heals_history` |
| Independent hot recovery and cold large-leap recovery | `gate_b_acceptance_02_independent_hot_and_cold_large_leap_recovery` | — |
| Two-wound final-control gating and distinct healing | `gate_b_acceptance_03_two_wound_final_control_gates_distinct_healing` | `distinct_wounds_bleed_and_snapshot_deterministically` |
| Successful shock care while another wound bleeds | `gate_b_acceptance_04_shock_care_while_bleeding_never_starts_recovery` | `shock_treatment_rejects_an_unaffected_patient_without_consumption` |
| Nonfatal recovery cancellation and absolute-deadline restart | `gate_b_acceptance_05_new_wound_cancels_and_restarts_absolute_recovery` | `wound_added_at_nonzero_clock_never_bleeds_retroactively` |
| Immediate wound and active-endpoint interruption matrices | `gate_b_acceptance_06_immediate_and_endpoint_interruption_matrix` | `immediate_wounds_emit_complete_causal_arrays` |
| Authoritative hot/cold command failure atomicity | `gate_b_acceptance_07_hot_cold_command_failure_atomicity` | — |
| Automatic medic/patient interruption and death precedence | `gate_b_acceptance_08_automatic_medic_patient_interruption_and_death_precedence` | `same_timestamp_hemorrhage_defeats_completion_and_cleans_relationship` |
| Real-entity availability churn and exact candidate visits | `gate_b_acceptance_09_real_entity_medic_churn_has_exact_visits` | — |
| Medic and patient despawn materialization and indexed cleanup | `gate_b_acceptance_10_medic_patient_despawn_materializes_and_cleans` | `active_removal_emits_interruption_then_removal_and_reuse_is_clean` |
| Hot/cold cycles during treatment and recovery | `gate_b_acceptance_11_hot_cold_cycles_preserve_treatment_and_recovery` | `sparse_boundary_across_empty_hot_cell_keeps_snapshot_accounting_valid` |
| Rollback after recovery/healing/availability/interruption commits | `gate_b_acceptance_12_rollback_restores_recovery_healing_and_availability` | — |
| Combined active-treatment/recovery snapshot continuation and corruption rejection | `gate_b_acceptance_13_active_treatment_recovery_snapshot_and_corruption` | — |
| Bleeding/recovery query purity, including indexes and counters | `gate_b_acceptance_14_bleeding_recovery_queries_preserve_authority` | `query_permutations_during_bleeding_and_recovery_are_pure` |

Gate C, exhaustive protocol/persistence matrices, representative medical benchmark expansion, production review, and M1.3 remain out of scope.
