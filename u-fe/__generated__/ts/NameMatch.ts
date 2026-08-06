/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<a6f1fba8136e126f1c4b53de5e61b792>>
 */


import type { NameMatchMode } from './NameMatchMode.ts';

/** What a node's name has to look like. */
export interface NameMatch {
  pattern: string;
  mode: NameMatchMode;
}