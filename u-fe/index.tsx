// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createRoot } from "react-dom/client";
import { useCallback, useEffect, useMemo, useState } from "react";

import type { PageParams } from "./PageParams";
import { Explorer, type InputGraph } from "./Explorer";

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
  const [tvcUrlParam, setTvcUrlParam] = useState<string | null>(() =>
    getQueryParam("tvc"),
  );

  useEffect(() => {
    const urlHandler = () => {
      const newTvcUrlParam = getQueryParam("tvc");
      setTvcUrlParam(newTvcUrlParam);
    };

    window.addEventListener("popstate", urlHandler);
    return () => {
      window.removeEventListener("popstate", urlHandler);
    };
  }, []);

  const array_graph_json_zstd_base64 = useMemo(() => {
    const array_graph_json_zstd_base64_Element = document.getElementById(
      "array_graph_json_zstd_base64",
    );
    if (array_graph_json_zstd_base64_Element == null) {
      throw new Error("Array graph JSON element not found");
    }
    const arrayGraphJSON = array_graph_json_zstd_base64_Element.textContent;
    if (arrayGraphJSON == null) {
      throw new Error("Array graph JSON is null");
    }
    return arrayGraphJSON;
  }, []);

  const onTraversalConfigZSTDBase64UrlSafeNoPaddingChange = useCallback(
    (newTvcUrlParam: string) => {
      if (newTvcUrlParam === tvcUrlParam) {
        return; // No change, do nothing
      }
      setTvcUrlParam(newTvcUrlParam);
      const url = new URL(window.location.href);
      url.searchParams.set("tvc", newTvcUrlParam);
      window.history.pushState({}, "", url.toString());
    },
    [tvcUrlParam],
  );

  const graph: InputGraph = useMemo(() => {
    return {
      t: "array_graph_json_zstd_base64",
      array_graph_json_zstd_base64,
    };
  }, [array_graph_json_zstd_base64]);

  return (
    <Explorer
      onPageParamsChange={updateURLParams}
      pageParams={initialParams}
      traversalConfigZSTDBase64UrlSafeNoPadding={tvcUrlParam}
      onTraversalConfigZSTDBase64UrlSafeNoPaddingChange={
        onTraversalConfigZSTDBase64UrlSafeNoPaddingChange
      }
      graph={graph}
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
  const params = getQueryParam("page_params");
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

function getQueryParam(name: string): string | null {
  const url = new URL(window.location.href);
  const value = url.searchParams.get(name);
  return value;
}
