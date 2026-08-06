/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<5b99fcf70ce88cdcf53432a89d3f9e94>>
 */


import type { PropertyValueMatch } from './PropertyValueMatch.ts';

/**
 * Conditions that narrow the flat list down to a subset of nodes.
 * 
 * Used in combination with `ArrayGraphUISettingsTreeTableEntryPoints::Filtered`.
 * A node matches only when it satisfies every condition — this is an AND
 * across all the fields and across the entries within each of them.
 */
export interface EntryPointsFilter {
  /** Property name -> what the value has to look like. */
  properties: { [key: string]: PropertyValueMatch };
  /** Node must have an incoming edge tagged with each of these. */
  incoming_tags: string[];
  /** Node must have an incoming dynamic edge with each of these type keys. */
  incoming_dynamic_type_keys: string[];
  /** Node must have an outgoing edge tagged with each of these. */
  outgoing_tags: string[];
  /** Node must have an outgoing dynamic edge with each of these type keys. */
  outgoing_dynamic_type_keys: string[];
}