// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GraphSettings } from "@/__generated__/ts/GraphSettings";
import { useMetricViewState } from "./MetricViewStateContext";

export type GraphSettingsContextType = [
  GraphSettings,
  (settings: GraphSettings) => void,
];

export function useGraphSettings(): GraphSettingsContextType {
  const { graphSettings, setGraphSettings } = useMetricViewState();
  return [graphSettings, setGraphSettings];
}
