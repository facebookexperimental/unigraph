/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<939d839d865c0c3871cb5f44a9e3dd8d>>
 */


export interface AscendingTier {
  name: string;
  /** A tagged edge with any of these tags bumps its target node to this tier. */
  tags_that_transition_to_this_tier: string[];
  /**
   * A dynamic edge with any of these `DynamicTypeKey`s (e.g. `"rc:gk"`) bumps
   * its target node to this tier — the dynamic-edge analog of
   * `tags_that_transition_to_this_tier`. Defaulted so older serialized graphs
   * (which predate this field) still deserialize.
   */
  dynamic_type_keys_that_transition_to_this_tier: string[];
}