// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { Explorer } from "./Explorer";
import type { ExplorerComponentInputGraph } from "./__generated__/ts/ExplorerComponentInputGraph";
import type { ExplorerComponentInputGraphs } from "./__generated__/ts/ExplorerComponentInputGraphs";

const ARRAY_GRAPH_JSON_ZSTD_BASE64_LEFT_ELEMENT_ID =
  "array_graph_json_zstd_base64_left";
const ARRAY_GRAPH_JSON_ZSTD_BASE64_RIGHT_ELEMENT_ID =
  "array_graph_json_zstd_base64_right";

const QUERY_PARAM_GRAPH_SETTINGS = "graph_settings";
const QUERY_PARAM_TVC_L = "tvc";
const QUERY_PARAM_TVC_R = "tvc_r";

window.onload = () => {
  const rootDiv = document.getElementById("root");
  if (rootDiv == null) {
    throw new Error("Root element not found");
  }

  const root = createRoot(rootDiv);
  root.render(<Root />);
};

function Root() {
  const [tvcUrlParamL, setTvcUrlParamL] = useState<string | null>(() =>
    getQueryParam(QUERY_PARAM_TVC_L),
  );
  const [tvcUrlParamR, setTvcUrlParamR] = useState<string | null>(() =>
    getQueryParam(QUERY_PARAM_TVC_R),
  );
  const [graphSettingsURLParam, setGraphSettingsURLParam] = useState<
    string | null
  >(() => getQueryParam(QUERY_PARAM_GRAPH_SETTINGS));

  useEffect(() => {
    const urlHandler = () => {
      const newTvcUrlParamL = getQueryParam(QUERY_PARAM_TVC_L);
      const newTvcUrlParamR = getQueryParam(QUERY_PARAM_TVC_R);
      setTvcUrlParamL(newTvcUrlParamL);
      setTvcUrlParamR(newTvcUrlParamR);
    };

    window.addEventListener("popstate", urlHandler);
    return () => {
      window.removeEventListener("popstate", urlHandler);
    };
  }, []);

  const graphs: ExplorerComponentInputGraphs = useMemo(() => {
    const left = getSerializedGraphFromHTMLElement(
      ARRAY_GRAPH_JSON_ZSTD_BASE64_LEFT_ELEMENT_ID,
    );
    const right = getSerializedGraphFromHTMLElement(
      ARRAY_GRAPH_JSON_ZSTD_BASE64_RIGHT_ELEMENT_ID,
    );

    if (left == null) {
      throw new Error("Left graph must be present");
    }
    return {
      left,
      right: right ?? undefined,
    };
  }, []);

  const on_traversal_config_change_l = useCallback(
    (newTvcUrlParamL: string) => {
      if (newTvcUrlParamL === tvcUrlParamL) {
        return; // No change, do nothing
      }
      setTvcUrlParamL(newTvcUrlParamL);
      const url = new URL(window.location.href);
      url.searchParams.set(QUERY_PARAM_TVC_L, newTvcUrlParamL);
      window.history.pushState({}, "", url.toString());
    },
    [tvcUrlParamL],
  );

  const on_traversal_config_change_r = useCallback(
    (newTvcUrlParamR: string) => {
      if (newTvcUrlParamR === tvcUrlParamR) {
        return; // No change, do nothing
      }
      setTvcUrlParamR(newTvcUrlParamR);
      const url = new URL(window.location.href);
      url.searchParams.set(QUERY_PARAM_TVC_R, newTvcUrlParamR);
      window.history.pushState({}, "", url.toString());
    },
    [tvcUrlParamR],
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

  return (
    <Explorer
      graphs={graphs}
      traversal_config_l={tvcUrlParamL ?? undefined}
      on_traversal_config_change_l={on_traversal_config_change_l}
      traversal_config_r={tvcUrlParamR ?? undefined}
      on_traversal_config_change_r={on_traversal_config_change_r}
      graph_settings={graphSettingsURLParam ?? undefined}
      on_graph_settings_change={onGraphSettingsZSTDBase64UrlSafeNoPaddingChange}
    />
  );
}

function getQueryParam(name: string): string | null {
  const url = new URL(window.location.href);
  const value = url.searchParams.get(name);
  return value;
}

function getSerializedGraphFromHTMLElement(
  elementID: string,
): ExplorerComponentInputGraph | null {
  const array_graph_json_zstd_base64_Element =
    document.getElementById(elementID);

  if (array_graph_json_zstd_base64_Element == null) {
    throw new Error(
      `Array graph JSON element not found. elementID: ${elementID}`,
    );
  }

  const content = array_graph_json_zstd_base64_Element.textContent;

  if (content === "" || content == null) {
    return null;
  }

  return JSON.parse(content) as ExplorerComponentInputGraph;
}
