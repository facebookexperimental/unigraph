/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<5eeba657ca9a735b1aa9db2ac452bad1>>
 */


/**
 * How a node-name pattern is read.
 * 
 * `Substring` and `Regex` are predicates — every node's name is tested against
 * them. `Exact` and `Fuzzy` are generators: they produce candidates directly
 * from the name list, which is why the evaluator can seed from them instead of
 * scanning. See the module docs on `select_nodes` for what that means for
 * ordering and for the interaction between `Fuzzy` and the other conditions.
 */
export type NameMatchMode = "Substring" | "Regex" | "Fuzzy" | "Exact";