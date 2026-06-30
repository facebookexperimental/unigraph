/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<fffe7df23fc119deb7eddda48e69943f>>
 */


/**
 * Enum that defines whether an option is enabled or not depending
 * on whether the right graph is present or not.
 * For example, when we have `changed nodes only` enabled it has no
 * meaning in the context of a single graph. This option provides
 * extra safety to make sure we don't accedentally pass `true` in
 * cases that are invalid.
 */
export type OptionEnabledDependingOnRightGraph = "WhenRightGraphPresent" | "Never";