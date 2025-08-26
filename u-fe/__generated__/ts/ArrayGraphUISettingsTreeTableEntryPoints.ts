/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

/**
 * Will be used as entry points for the tree table.
 * Otherwise we will use the determined entry points.
 * This is needed for things like: show as flat list, show selected nodes,
 * show reverse from a specific node, etc.
 */
export type ArrayGraphUISettingsTreeTableEntryPoints =
  | "Determine"
  | "AllReachable"
  | "Specified";
