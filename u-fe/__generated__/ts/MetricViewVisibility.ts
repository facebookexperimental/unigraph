/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Controls when a metric view column is shown in the UI.
 * 
 * This is the visibility layer — it only applies to views that are
 * already available (per `MetricsConfig`). Availability and visibility
 * are separate concerns.
 */
export type MetricViewVisibility = "Enabled" | "EnabledInDominatorMode" | "Hidden";