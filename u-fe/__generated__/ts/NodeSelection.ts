/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<88961999c21b38152f3c1c3846ac53da>>
 */


import type { NameMatch } from './NameMatch.ts';
import type { PropertyValueMatch } from './PropertyValueMatch.ts';

/**
 * Conditions that narrow the graph down to a subset of nodes.
 * 
 * A node matches only when it satisfies every condition — this is an AND
 * across all the fields and across the entries within each of them.
 */
export interface NodeSelection {
  /** Node name must match this. Absent — or blank — matches every name. */
  name?: NameMatch | undefined;
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