/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<6a4de09e6533dbaac55db064d678cc7a>>
 */


import type { GraphHandle } from './GraphHandle.ts';

export interface FindPathInput {
  /** Graph handle — timeline ID, graph key, or GQC key. */
  handle: GraphHandle;
  /** Starting node name. */
  from: string;
  /** Target node name. */
  to: string;
  /** When true, include a human-readable ASCII summary in the response. */
  include_ascii?: boolean | undefined;
}