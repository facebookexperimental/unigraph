/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<ece6d259c4a57f2ffe20a265aae82bf4>>
 */


/**
 * An edge as a pair of node names.
 * 
 * The name-space counterpart of `unigraph_core::MinCutEdge`, which is
 * `NodeIDX`-based: indices are meaningless to an RPC caller, who never sees the
 * graph's index space.
 */
export interface MinCutNamedEdge {
  from: string;
  to: string;
}