# M1.1 contract-to-test map

This map names the concrete fixtures used for each numbered cluster in
`CODEX_TASK_M1_1.md`. Test names are intentionally descriptive; a cluster name
alone is not treated as evidence.

| Cluster | Concrete tests and fixtures |
| --- | --- |
| 1. Independent reference/cold matrix | `private_invariants::independent_transition_oracle_threshold_matrix` (15 supplemental pure-transition rows); `private_invariants::cold_and_hot_paths_each_match_hand_authored_boundary_fixtures` (8 path-level fixtures); `cluster_01_one_second_oracle_has_independent_expected_values`; `oracle_matrix_duration_zero_is_an_exact_noop`; `cluster_11_huge_sparse_advance_has_exact_boundary_work` |
| 2. Hot/cold boundaries | `private_invariants::cold_and_hot_paths_each_match_hand_authored_boundary_fixtures` runs ration, forced-idle, deterioration, and death expectations separately through cold and hot advancement; `cluster_02_hot_and_cold_match_independent_boundaries` remains supplemental cross-path parity coverage |
| 3. Fidelity churn and restore | `cluster_03_repeated_fidelity_churn_preserves_state`; `fidelity_cycles_through_death_never_resurrect_or_double_work`; `cluster_09_hot_snapshot_and_canonical_due_are_strict` |
| 4. Conservation | `cluster_04_consumption_removal_and_ledger_are_conserved_once`; `removal_loss_survives_snapshot_restore_conservation`; `private_invariants::later_ledger_overflow_rolls_back_earlier_ration_and_events` |
| 5. Exhaustion and death | `exhaustion_causes_have_exact_independent_times` (hunger-only, thirst-only, and both); death rows in `independent_transition_oracle_threshold_matrix`; `cluster_11_huge_sparse_advance_has_exact_boundary_work` |
| 6. Activity behavior | `cluster_06_activities_differ_and_invalid_activity_is_atomic`; Rest/Idle/March and forced-idle rows in `independent_transition_oracle_threshold_matrix`; `private_invariants::hot_activity_never_installs_a_cold_due_entry` |
| 7. Due-index churn | `private_invariants::repeated_due_churn_keeps_exact_canonical_entries` (64 remove/reuse/fidelity cycles with private bucket/reverse inspection); `cluster_07_stale_generation_cannot_act_on_reused_soldier`; `private_invariants::stale_due_entry_cannot_execute_after_generation_reuse` |
| 8. Same-time ordering | `cluster_08_automatic_work_precedes_same_time_scheduled_commands`; `scheduled_commands_with_same_time_keep_schedule_id_order`; `private_invariants::automatic_events_are_globally_entity_ordered_and_dead_hot_members_are_not_work` |
| 9. Snapshot validation/query purity | `private_invariants::v6_byte_corruption_matrix_rejects_every_living_class` (38 named byte/encoded-field mutations with exact `SimError` categories); `cluster_09_hot_snapshot_and_canonical_due_are_strict`; `query_permutations_are_snapshot_and_digest_pure`; existing M0 corruption tests in `sim-core/tests/invariants.rs` |
| 10. Replay/continuation | `private_invariants::combined_membership_reuse_death_restore_and_replay_fixture` pins future events, reused ID, complete member state/counters, and final digest for uninterrupted and restored runs; `cluster_10_queries_and_snapshot_resume_are_byte_deterministic`; `repeated_restore_bytes_are_identical` |
| 11. Sparse structural work | `private_invariants::sparse_candidate_visits_ignore_population_times_segments` separately asserts 0 journal/execution visits for 2,000 far-future cold soldiers across 49 unrelated schedule segments, then exactly 2,000 journal and 2,000 execution visits at the due segment, zero for an unrelated later segment, and +7/+7 for a small due cohort; `cluster_11_huge_sparse_advance_has_exact_boundary_work` proves huge elapsed-time boundary counts |
| 12. Exact membership | `private_invariants::combined_membership_reuse_death_restore_and_replay_fixture` combines two hot cells, removal, slot reuse in another cell, death, cold exclusion, restore, exact events, and exact step counts; `cluster_12_hot_processing_touches_exact_indexed_membership` |

Rollback and defensive arithmetic are additionally covered by
`same_timestamp_cold_counter_overflow_commits_nothing`,
`complete_hot_segment_rolls_back_all_internal_seconds`,
`complete_cold_segment_rolls_back_all_internal_boundaries`,
`later_ledger_overflow_rolls_back_earlier_ration_and_events`, and
`failed_later_segment_preserves_earlier_scheduled_prefix`. These private tests
distinguish deliberately constructed defensive states from snapshots accepted
by `World::from_snapshot`.

`consumed_food_and_fixed_step_defenses_roll_back_complete_state` adds the
consumed-food and hot fixed-step defensive dimensions and labels states that
cannot be produced by a valid, conservation-consistent v6 snapshot.
