/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<6ffcd8db0ab10d06c7b469736fdf2cf6>>
 */


import type { FrameError } from './FrameError.ts';

/** The error payload stored on an `Error` frame. */
export interface FrameErrorInfo {
  error_count: number;
  errors: FrameError[];
}