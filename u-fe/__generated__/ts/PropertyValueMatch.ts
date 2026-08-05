/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<68693ac69c44af65020a4eb5c286f6f7>>
 */


/**
 * What a property condition requires of a node's value for that property.
 * 
 * A struct rather than a bare `Option<PropertyValue>` because this is a map
 * value: `JSON.stringify` drops `undefined`, so an optional-valued map entry
 * would silently disappear on the way back from the UI. An empty object
 * survives the round trip and leaves room for future match modes.
 */
export interface PropertyValueMatch {
  /**
   * Required exact value. Absent matches any node carrying the property,
   * whatever its value.
   */
  value?: string | undefined;
}