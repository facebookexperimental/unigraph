// Copyright (c) Meta Platforms, Inc. and affiliates.

import { P, Pre } from "../Typography";

export default function ConjointCostDocs() {
  return (
    <div className="flex flex-col gap-4">
      <P
        text="
Conjoint cost of a node is a value that represents its transitive size
        adjusted for how many other nodes it depends on.
      "
      />
      <P
        text="
        It's calculated by summing up the cost of all ConjCost(direct children)
        and dividing it by the number of parents.
      "
      />
      <Pre
        text={`conj(A) = (
    1_for_self +
    A.children.map(
      child -> conj(child)
    ).sum()
) / A.parents.length`}
      />

      <P
        text="
This metric will be lower compared to regular transitive size for things
that are popular. E.g. if there is a popular framework that almost every
single node uses it would not make sense for it to try to remove that dependency,
since it will likely still stay in the graph.
      "
      />
      <P
        text="
Best way to use this metric is to show the graph as a flat list, order
by conjoint cost 'descending' and then look for nodes that have high
conjoint cost but don't seem like they should be there.
      "
      />
    </div>
  );
}
