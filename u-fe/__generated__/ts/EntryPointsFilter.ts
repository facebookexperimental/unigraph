/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<0bffba2c1cb0e5cd79832c84caf42dc2>>
 */


import type { PropertyValueMatch } from './PropertyValueMatch.ts';

/**
 * Conditions that narrow the flat list down to a subset of nodes.
 * 
 * Used in combination with `ArrayGraphUISettingsTreeTableEntryPoints::Filtered`.
 * A node matches only when it satisfies every condition — this is an AND
 * across the three fields and across the entries within each of them.
 */
export interface EntryPointsFilter {
  /** Property name -> what the value has to look like. */
  properties: { [key: string]: PropertyValueMatch };
  /** Node must have an incoming edge tagged with each of these. */
  incoming_tags: string[];
  /** Node must have an incoming dynamic edge with each of these type keys. */
  incoming_dynamic_type_keys: string[];
}