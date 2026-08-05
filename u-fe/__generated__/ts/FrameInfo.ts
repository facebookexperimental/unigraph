/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<74f951d151bb3e833508b2919fa083f1>>
 */


import type { FrameErrorInfo } from './FrameErrorInfo.ts';

export interface FrameInfo {
  graph_id: number;
  timestamp: string;
  frame_type: string;
  base?: string | undefined;
  /**
   * Resolved error content, only for `Error` frames and only when the
   * request set `include_error_info`.
   */
  error?: FrameErrorInfo | undefined;
}