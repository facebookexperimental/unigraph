/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Defines how to dynamically generate budget definitions from the graph.
 * 
 * Dynamic definitions are resolved at compute time by inspecting the source
 * graph, then merged with static `BudgetConfig.budgets` (static wins on
 * name conflicts).
 */
export type DynamicBudgetDefinition =
  /**
   * Create one budget per entry point in the graph.
   * 
   * Entry points are determined by [`ArrayGraph::determine_entrypoints()`]
   * (parentless nodes, or explicit `entry_points` if set on the graph).
   * Each budget gets the entry point's node name as its budget name and
   * a single-element `entry_points` set.
   */
  { "AllEntryPoints": {  } };

export type DynamicBudgetDefinitionVariants = "AllEntryPoints";