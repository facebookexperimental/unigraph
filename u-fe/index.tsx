// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createRoot } from "react-dom/client";
import { useMemo } from "react";

import type { PageParams } from "./PageParams";
import { Explorer } from "./Explorer";

window.onload = () => {
  const rootDiv = document.getElementById("root");
  if (rootDiv == null) {
    throw new Error("Root element not found");
  }

  const root = createRoot(rootDiv);
  root.render(<Root />);
};

function Root() {
  const initialParams = useMemo(() => parseURLParams(), []);

  const mapGraphJSON = useMemo(() => {
    const mapGraphJSONElement = document.getElementById("map-graph-json");
    if (mapGraphJSONElement == null) {
      throw new Error("Map graph JSON element not found");
    }
    const mapGraphJSON = mapGraphJSONElement.textContent;
    if (mapGraphJSON == null) {
      throw new Error("Map graph JSON is null");
    }
    return mapGraphJSON;
  }, []);

  return (
    <Explorer
      onPageParamsChange={updateURLParams}
      initialPageParams={initialParams}
      graph={{ t: "MapGraphJSON", mapGraphJSON }}
    />
  );
}

function updateURLParams(params: PageParams) {
  const url = new URL(window.location.href);
  const json = JSON.stringify(params);
  url.searchParams.set("page_params", json);
  window.history.pushState({}, "", url.toString());
}

function parseURLParams(): PageParams {
  const url = new URL(window.location.href);
  const params = url.searchParams.get("page_params");
  if (params == null) {
    return {};
  }
  try {
    const parsedParams = JSON.parse(params);
    if (typeof parsedParams !== "object") {
      throw new Error("Parsed params is not an object");
    }
    return parsedParams;
  } catch (e) {
    console.error("Failed to parse URL params", e);
    return {};
  }
}
