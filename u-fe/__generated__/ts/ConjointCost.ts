/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

/**
 * Conjoint cost of the component is a value that represents its transitive
 * size adjusted for how many other nodes it depends on.
 * It's calculated by summing up the cost of all ConjCost(direct children) and
 * dividing it by the number of parents.
 *
 * This way people will be penalized less for things that are popular.
 * E.g. if there is a popular framework that almost every single node
 * uses it would not make sense for it to try to remove that depenedncy, since
 * it will likely still stay in the graph.
 */
export interface ConjointCost {
  count: number[];
  metrics: { [key: string]: number[] };
  tiered_metric: { [key: string]: { [key: string]: number[] } };
}
