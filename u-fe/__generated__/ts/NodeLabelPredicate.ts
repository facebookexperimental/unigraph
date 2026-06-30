/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<db9eaac3ebf3d6d248060ce5aafddd62>>
 */


import type { Decision } from './Decision.ts';

/**
 * These predicates are used to decide whether to follow an edge to a node based
 * on node's labels, which will contain some annotations about the node.
 * 
 * Specifically there are two concepts here:
 *        @assert_value v1 v2 v3: if the label is present, we ONLY follow the edge
 *          if the label contains a passed value (set globally). Otherwise we do not
 *          follow the edge.
 *        @disallow_value v1 v2 v3: if the label is present, we do NOT follow the edge
 *          if the label contains a passed value (set globally). Otherwise we do follow
 *          the edge (unless other predicates disallow it).
 * 
 * assuming current route is "homepage".
 * this produces these predicates:
 * 
 * [
 *    { label_name: "assert_route", label_value: "homepage", contains: true, decision: { include: true } },
 *    { label_name: "assert_route", label_value: "homepage", contains: false, decision: { include: false } },
 *    { label_name: "disallow_route", label_value: "homepage", contains: true, decision: { include: false } },
 *    { label_name: "disallow_route", label_value: "homepage", contains: false, decision: { include: true } },
 * ]
 */
export interface NodeLabelPredicate {
  label_name: string;
  label_value: string;
  contains: boolean;
  decision: Decision;
}