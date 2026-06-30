/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<a8bfe5bb7360155e828f5e2df498cba5>>
 */


/**
 * Represents an edge that can point to multiple nodes with branches,
 * as well as have metadata associated with it.
 */
export interface DynamicEdge {
  branches: { [key: string]: string[] };
  metadata?: { [key: string]: string } | undefined;
}