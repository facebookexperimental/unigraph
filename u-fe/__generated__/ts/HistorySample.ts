/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<e5e7f83b4b382931d2dbe75a742cb689>>
 */


export interface HistorySample {
  /** Index into [`GetHistoryOutput::frames`]. */
  frame: number;
  /**
   * Values aligned with [`GetHistoryOutput::metrics`]. `null` where the
   * node had no value for that metric at this frame; all-null means the
   * node was absent from the frame.
   */
  values: (number | undefined)[];
}