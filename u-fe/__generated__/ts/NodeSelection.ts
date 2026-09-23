/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<97e2c62f392ac21acaa2718ba8367910>>
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
  /**
   * Metric name -> the value a node's metric has to equal.
   * 
   * Compared as integers, never as floats. A node's `f64` is rounded the
   * same way [`MetricFormat::format_value`] rounds it to choose an `Enum`
   * label, so a condition asks "does this render as that variant?" rather
   * than "is this bitwise equal to that float?". Those two agree for an enum
   * and would not for a float equality test, which is the whole reason this
   * is an `i64`: matching what the table shows is the only useful contract,
   * and it takes float comparison out of the picture entirely.
   * 
   * That makes it a poor fit for a continuous metric — equalling a byte
   * count exactly is rarely the question — so this exists for the enum and
   * boolean formats. A range predicate would be a separate condition.
   * 
   * Unlike [`properties`](Self::properties) the value is not optional:
   * metrics are stored densely and default to `0.0`, so every node "has"
   * every metric and "carries this at all" is not a question worth asking.
   * 
   * [`MetricFormat::format_value`]: crate::graph_settings::MetricFormat::format_value
   */
  metrics: { [key: string]: number };
  /** Node must have an incoming edge tagged with each of these. */
  incoming_tags: string[];
  /** Node must have an incoming dynamic edge with each of these type keys. */
  incoming_dynamic_type_keys: string[];
  /** Node must have an outgoing edge tagged with each of these. */
  outgoing_tags: string[];
  /** Node must have an outgoing dynamic edge with each of these type keys. */
  outgoing_dynamic_type_keys: string[];
}