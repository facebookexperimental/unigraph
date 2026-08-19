/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<4f8085fee5c3461b9f86a869b1038e49>>
 */


/**
 * An override of *which* graph a metric view reads from, when a table is
 * comparing two.
 * 
 * Deliberately has no `Right` variant: a view's side is optional, and its
 * absence means the primary ("after") graph — the only graph outside delta
 * mode. So the common case has exactly one representation. Single-graph code
 * leaves the side unset, JSON omits the field entirely, and there is no
 * second spelling that would render identically yet compare unequal in the
 * maps these views key.
 */
export type MetricSide = "Left" | "Delta";