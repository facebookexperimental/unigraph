/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0e338419a1ed97ed5ae8eefaae41e28e>>
 */


/**
 * Will be used as entry points for the tree table.
 * Otherwise we will use the determined entry points.
 * This is needed for things like: show as flat list, show selected nodes,
 * show reverse from a specific node, etc.
 * 
 * Every variant must stay a unit variant — the Hack typegen backend rejects
 * enums that mix unit and data variants. Variants that need a payload store
 * it in a sibling field on `ArrayGraphUISettings` (see `Specified` /
 * `entry_points_specified` and `Filtered` / `entry_points_filter`).
 */
export type ArrayGraphUISettingsTreeTableEntryPoints = "Determine" | "AllReachable" | "Specified" | "Filtered";