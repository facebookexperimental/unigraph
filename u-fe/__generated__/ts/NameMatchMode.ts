/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<b8fa37a7b5ac245ed7cb3f4f9ac244ef>>
 */


/**
 * How a node-name pattern is read.
 * 
 * Distinct from `unigraph_app`'s `SearchMode`, which drives the typeahead:
 * that one is subsequence-fuzzy and top-K, where a filter needs every match
 * and a predicate the user can reason about exactly.
 */
export type NameMatchMode = "Substring" | "Regex";