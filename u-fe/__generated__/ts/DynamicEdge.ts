/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Represents an edge that can point to multiple nodes with branches,
 * as well as have metadata associated with it.
 */
export interface DynamicEdge {
  branches: { [key: string]: string[] };
  metadata?: { [key: string]: string } | undefined;
}