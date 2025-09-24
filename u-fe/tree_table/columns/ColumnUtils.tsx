// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphStructure } from "../../__generated__/ts/GraphStructure";
import type { IndividualDominatedOptionEnabled } from "../../__generated__/ts/IndividualDominatedOptionEnabled";

export function isEnabledForGraphStructure(
  graphStructure: GraphStructure = "Forward",
  value: IndividualDominatedOptionEnabled | undefined,
) {
  if (graphStructure === "Dominator") {
    return (
      value == null ||
      value === "WhenEnabledGlobally" ||
      value === "WhenEnabledGloballyAndInDominatorMode"
    );
  } else {
    return value === "WhenEnabledGlobally";
  }
}
