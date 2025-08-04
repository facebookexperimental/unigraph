import type { ArrayGraphUISettingsTreeTableEntryPoints } from './ArrayGraphUISettingsTreeTableEntryPoints.ts';
import type { ColumnSettings } from './ColumnSettings.ts';
import type { GraphStructure } from './GraphStructure.ts';
import type { GraphTableSort } from './GraphTableSort.ts';
import type { SidebarPanel } from './SidebarPanel.ts';

export interface ArrayGraphUISettings {
  selected_sidebar_panel?: SidebarPanel | undefined;
  graph_table_sort?: GraphTableSort | undefined;
  columns?: ColumnSettings | undefined;
  graph_structure?: GraphStructure | undefined;
  entry_points?: ArrayGraphUISettingsTreeTableEntryPoints | undefined;
  /** Used in combination with `entry_points` settings.` If entry_points is set to `Specified`, this value will be used to determine entry points. This value is stored separately so we can preserve selected entry points when switching between different entry points settings. E.g. if we're exploring `reverse from a specific node` and want to hop into `show as flat list`, we want to preserve the selected entry points, so when we switch back to "reverse" we keep the same selected entry point */
  entry_points_specified?: string[] | undefined;
}