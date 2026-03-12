/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * A single budget within a [`BudgetConfig`].
 * 
 * Each budget specifies entry points in the source graph that serve
 * as DFS roots for metric aggregation. The `properties` map provides
 * freeform key-value pairs for custom algorithm parameters (e.g.
 * `"comet_route_trace_policy"` for WWW route budgets).
 */
export interface BudgetDefinition {
  /** Node names in the source graph that serve as DFS roots. */
  entry_points: string[];
  /**
   * Algo-specific properties per budget definition.
   * Custom algos read what they need from here.
   */
  properties?: { [key: string]: string } | undefined;
}