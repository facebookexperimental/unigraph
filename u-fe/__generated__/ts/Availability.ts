/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<63c572f395b7a3dd7e3720af7660464e>>
 */


/**
 * Whether a metric view type can be computed in this graph.
 * 
 * Part of `MetricsConfig` — the data-level availability layer.
 * `Unavailable` means the view doesn't exist at all: it won't appear
 * in `available_metric_views()`, the `about` RPC, or the CLI.
 * Defaults to `Available` so that old graphs without `MetricsConfig`
 * keep all their views.
 */
export type Availability = "Available" | "Unavailable";