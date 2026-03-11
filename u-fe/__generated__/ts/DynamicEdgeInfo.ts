/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Dynamic-edge-only fields. None for directed/tagged edges.
 * Shared between Arrow (ArrayGraph level) and NamedArrow (MapGraph level).
 */
export interface DynamicEdgeInfo {
  type_key: string;
  edge_name: string;
  branch: string;
  metadata?: { [key: string]: string } | undefined;
}