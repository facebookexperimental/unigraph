// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";

import { Explorer } from "./Explorer";
import type { ExplorerComponentInputGraph } from "./__generated__/ts/ExplorerComponentInputGraph";

const ARRAY_GRAPH_JSON_ZSTD_BASE64_LEFT_ELEMENT_ID =
  "array_graph_json_zstd_base64_left";
const ARRAY_GRAPH_JSON_ZSTD_BASE64_RIGHT_ELEMENT_ID =
  "array_graph_json_zstd_base64_right";

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

  const [
    array_graph_json_zstd_base64_left,
    _array_graph_json_zstd_base64_right,
  ] = useMemo(() => {
    const left = getSerializedGraphFromHTMLElement(
      ARRAY_GRAPH_JSON_ZSTD_BASE64_LEFT_ELEMENT_ID,
    );
    const right = getSerializedGraphFromHTMLElement(
      ARRAY_GRAPH_JSON_ZSTD_BASE64_RIGHT_ELEMENT_ID,
    );

    if (left == null) {
      throw new Error("Left graph must be present");
    }
    return [left, right];
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

  const graph: ExplorerComponentInputGraph = useMemo(() => {
    return {
      ArrayGraphSerialized: {
        format: "JsonZstdBase64",
        value: array_graph_json_zstd_base64_left,
      },
    };
  }, [array_graph_json_zstd_base64_left]);

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

function getSerializedGraphFromHTMLElement(elementID: string): string | null {
  const array_graph_json_zstd_base64_Element =
    document.getElementById(elementID);

  if (array_graph_json_zstd_base64_Element == null) {
    throw new Error(
      `Array graph JSON element not found. elementID: ${elementID}`,
    );
  }

  return array_graph_json_zstd_base64_Element.textContent;
}
