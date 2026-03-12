/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { BudgetAlgoConfig } from './BudgetAlgoConfig.ts';
import type { BudgetDefinition } from './BudgetDefinition.ts';
import type { DynamicBudgetDefinition } from './DynamicBudgetDefinition.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

/**
 * Top-level budget configuration, stored on `ArrayGraph.budget_configs`
 * keyed by project name (e.g. "CometBudget", "CargoBudget").
 * 
 * Contains the algorithm choice, a map of named budget definitions,
 * and an optional traversal config to apply before computing.
 * 
 * Stored on the graph so it survives serialization, pack/unpack, and
 * delta round-trips. This makes graphs self-describing: given a graph
 * blob, you know what budgets to compute.
 */
export interface BudgetConfig {
  /** Which algorithm to use and its configuration. */
  algo: BudgetAlgoConfig;
  /** The budgets to compute. Key = budget name, value = definition. */
  budgets: { [key: string]: BudgetDefinition };
  /**
   * Dynamic budget definitions resolved from the graph at compute time.
   * Merged with `budgets` before computation (static budgets win conflicts).
   */
  dynamic_budget_definitions: { [key: string]: DynamicBudgetDefinition };
  /**
   * Traversal config to apply to the source graph before computing.
   * If None, uses the source graph's existing traversal config.
   */
  traversal_config?: TraversalConfig | undefined;
}