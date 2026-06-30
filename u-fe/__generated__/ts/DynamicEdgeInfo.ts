/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<ad4bd6cc248d0cb12a85a93e3ae0df7c>>
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