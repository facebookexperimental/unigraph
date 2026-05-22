/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { Decision } from './Decision.ts';

export interface ForceDynamic {
  from_node?: string | undefined;
  match_properties: { [key: string]: string };
  branch?: string | undefined;
  decision: Decision;
}
