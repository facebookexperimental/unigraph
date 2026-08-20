/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f8d08bab9bb9bf4bfe1e4e4370469c1e>>
 */


import type { ArrayGraphUISettingsTreeTableEntryPoints } from './ArrayGraphUISettingsTreeTableEntryPoints.ts';
import type { ColumnSettings } from './ColumnSettings.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { NodeSelection } from './NodeSelection.ts';
import type { OptionEnabledDependingOnRightGraph } from './OptionEnabledDependingOnRightGraph.ts';
import type { SidebarPanel } from './SidebarPanel.ts';

export interface ArrayGraphUISettings {
  selected_sidebar_panel?: SidebarPanel | undefined;
  columns?: ColumnSettings | undefined;
  graph_structure?: GraphStructure | undefined;
  /**
   * Only used in delta view when comparing two graphs.
   * This will compress paths that graph table renders
   * and only show nodes that have changed between the two graphs
   * while skipping a lot of nodes in between.
   * 
   * We want to find the CLOSEST (possibly not direct) children
   * in the transitive dependencies of the node so we can show changed
   * nodes graph only.
   * 
   * E.g. if we have two graphs we're comparing:
   * 
   * A          A
   *   B          B
   *     C          C
   *       D          F    <- D was removed and F was added
   * 
   * 
   * The actual change is hidden deep down in the node. We would want to skip
   * showing B because it has no changes, and only show C, D and F because they
   * 
   * A          A
   *   C          C
   *     D          F
   */
  show_changed_nodes_only?: OptionEnabledDependingOnRightGraph | undefined;
  /**
   * What nodes should we use as the "start" of the graph
   * when we render the table.
   */
  entry_points?: ArrayGraphUISettingsTreeTableEntryPoints | undefined;
  /**
   * Used in combination with `entry_points` settings.`
   * If entry_points is set to `Specified`, this value will be
   * used to determine entry points. This value is stored separately
   * so we can preserve selected entry points when switching
   * between different entry points settings.
   * E.g. if we're exploring `reverse from a specific node` and want
   * to hop into `show as flat list`, we want to preserve
   * the selected entry points, so when we switch back to "reverse"
   * we keep the same selected entry point
   */
  entry_points_specified?: string[] | undefined;
  /**
   * Used in combination with `entry_points` settings.
   * If entry_points is set to `Filtered`, these conditions narrow the flat
   * list down to the nodes that match them. Stored separately from
   * `entry_points` for the same reason as `entry_points_specified`: so the
   * conditions survive switching to another entry point mode and back.
   */
  entry_points_filter?: NodeSelection | undefined;
}