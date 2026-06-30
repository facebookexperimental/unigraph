/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<eaf55797b1d516576199c62938caa8a2>>
 */


/**
 * Controls when a metric view column is shown in the UI.
 * 
 * This is the visibility layer — it only applies to views that are
 * already available (per `MetricsConfig`). Availability and visibility
 * are separate concerns.
 */
export type MetricViewVisibility = "Enabled" | "EnabledInDominatorMode" | "Hidden";