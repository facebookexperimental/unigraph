/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


export interface FindPathInput {
  /** Graph handle — timeline ID, graph key, or GQC key. */
  handle: string;
  /** Starting node name. */
  from: string;
  /** Target node name. */
  to: string;
  /** When true, include a human-readable ASCII summary in the response. */
  include_ascii?: boolean | undefined;
}