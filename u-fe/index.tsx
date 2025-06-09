// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createRoot } from "react-dom/client";
import { useCallback, useEffect, useMemo, useState } from "react";

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
  const [tvcUrlParam, setTvcUrlParam] = useState<string | null>(() =>
    getQueryParam("tvc"),
  );
  const [graphSettingsURLParam, setGraphSettingsURLParam] = useState<
    string | null
  >(() => getQueryParam("graph_settings"));

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

  const onGraphSettingsZSTDBase64UrlSafeNoPaddingChange = useCallback(
    (newGraphSettingsUrlParam: string) => {
      if (newGraphSettingsUrlParam === graphSettingsURLParam) {
        return; // No change, do nothing
      }
      setGraphSettingsURLParam(newGraphSettingsUrlParam);
      const url = new URL(window.location.href);
      url.searchParams.set("graph_settings", newGraphSettingsUrlParam);
      window.history.pushState({}, "", url.toString());
    },
    [graphSettingsURLParam],
  );

  const graph: InputGraph = useMemo(() => {
    return {
      t: "array_graph_json_zstd_base64",
      array_graph_json_zstd_base64,
    };
  }, [array_graph_json_zstd_base64]);

  return (
    <Explorer
      traversalConfigZSTDBase64UrlSafeNoPadding={tvcUrlParam}
      onTraversalConfigZSTDBase64UrlSafeNoPaddingChange={
        onTraversalConfigZSTDBase64UrlSafeNoPaddingChange
      }
      graphSettingsZSTDBase64UrlSafeNoPadding={graphSettingsURLParam}
      onGraphSettingsZSTDBase64UrlSafeNoPaddingChange={
        onGraphSettingsZSTDBase64UrlSafeNoPaddingChange
      }
      graph={graph}
    />
  );
}

function getQueryParam(name: string): string | null {
  const url = new URL(window.location.href);
  const value = url.searchParams.get(name);
  return value;
}
