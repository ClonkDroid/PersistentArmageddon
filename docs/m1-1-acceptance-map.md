# M1.1 contract-to-test map

This map names the concrete fixtures used for each numbered cluster in
`CODEX_TASK_M1_1.md`. Test names are intentionally descriptive; a cluster name
alone is not treated as evidence.

| Cluster | Concrete tests and fixtures |
| --- | --- |
| 1. Independent reference/cold matrix | `private_invariants::independent_transition_oracle_threshold_matrix` (15 hand-authored one-second rows); `cluster_01_one_second_oracle_has_independent_expected_values`; `oracle_matrix_duration_zero_is_an_exact_noop`; `cluster_11_huge_sparse_advance_has_exact_boundary_work` |
| 2. Hot/cold boundaries | `cluster_02_hot_and_cold_match_independent_boundaries`, supplemented by the independent ration, forced-idle, deterioration, and death rows in `independent_transition_oracle_threshold_matrix` and `cluster_05_ration_and_deterioration_boundaries_are_exact` |
| 3. Fidelity churn and restore | `cluster_03_repeated_fidelity_churn_preserves_state`; `fidelity_cycles_through_death_never_resurrect_or_double_work`; `cluster_09_hot_snapshot_and_canonical_due_are_strict` |
| 4. Conservation | `cluster_04_consumption_removal_and_ledger_are_conserved_once`; `removal_loss_survives_snapshot_restore_conservation`; `private_invariants::later_ledger_overflow_rolls_back_earlier_ration_and_events` |
| 5. Exhaustion and death | `exhaustion_causes_have_exact_independent_times` (hunger-only, thirst-only, and both); death rows in `independent_transition_oracle_threshold_matrix`; `cluster_11_huge_sparse_advance_has_exact_boundary_work` |
| 6. Activity behavior | `cluster_06_activities_differ_and_invalid_activity_is_atomic`; Rest/Idle/March and forced-idle rows in `independent_transition_oracle_threshold_matrix`; `private_invariants::hot_activity_never_installs_a_cold_due_entry` |
| 7. Due-index churn | `cluster_07_stale_generation_cannot_act_on_reused_soldier`; `private_invariants::stale_due_entry_cannot_execute_after_generation_reuse`; `private_invariants::canonical_due_sparse_queries_and_corruption_are_proven` |
| 8. Same-time ordering | `cluster_08_automatic_work_precedes_same_time_scheduled_commands`; `scheduled_commands_with_same_time_keep_schedule_id_order`; `private_invariants::automatic_events_are_globally_entity_ordered_and_dead_hot_members_are_not_work` |
| 9. Snapshot validation/query purity | `cluster_09_hot_snapshot_and_canonical_due_are_strict`; `query_permutations_are_snapshot_and_digest_pure`; `private_invariants::health_is_one_projection_and_impossible_health_is_rejected`; `private_invariants::canonical_due_sparse_queries_and_corruption_are_proven`; existing snapshot corruption tests in `sim-core/tests/invariants.rs` |
| 10. Replay/continuation | `cluster_10_queries_and_snapshot_resume_are_byte_deterministic`; `repeated_restore_bytes_are_identical`; `same_seed_commands_produce_identical_events_and_digest` |
| 11. Sparse structural work | `cluster_11_huge_sparse_advance_has_exact_boundary_work`; `private_invariants::canonical_due_sparse_queries_and_corruption_are_proven`; bounded/full benchmark structural counters |
| 12. Exact membership | `cluster_12_hot_processing_touches_exact_indexed_membership`; `multiple_hot_cells_step_only_living_indexed_members_in_id_order`; `fidelity_cycles_through_death_never_resurrect_or_double_work` |

Rollback and defensive arithmetic are additionally covered by
`same_timestamp_cold_counter_overflow_commits_nothing`,
`complete_hot_segment_rolls_back_all_internal_seconds`,
`complete_cold_segment_rolls_back_all_internal_boundaries`,
`later_ledger_overflow_rolls_back_earlier_ration_and_events`, and
`failed_later_segment_preserves_earlier_scheduled_prefix`. These private tests
distinguish deliberately constructed defensive states from snapshots accepted
by `World::from_snapshot`.
