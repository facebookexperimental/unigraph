/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Selects which algorithm to use for budget graph computation.
 * 
 * Uses `#[deltable(replace)]` — the entire enum value is replaced
 * in deltas rather than attempting per-field diffing across variants.
 */
export type BudgetAlgoConfig =
  /** Built-in: transitive aggregation from entry points. */
  { "Transitive": { metrics: string[], counts: boolean, tiered_metrics: string[] } } |
  /**
   * Custom algorithm, looked up by name in the registry
   * passed to build_budget_graph_with_custom_algos().
   */
  { "Custom": { name: string } };

export type BudgetAlgoConfigVariants = "Transitive" | "Custom";