// Copyright (c) Meta Platforms, Inc. and affiliates.

import { describe, expect, it, vi } from "vitest";
import type { ExplorerUrlParams } from "../../__generated__/ts/ExplorerUrlParams";
import {
  EXPLORER_URL_PARAM_KEYS,
  parseExplorerUrlParams,
  readExplorerUrlParams,
  resolveOverrides,
  serializeExplorerUrlParams,
} from "../explorerUrlParams";

describe("resolveOverrides", () => {
  // Mirrors the snapshot table in explorer_url_params.rs — the two
  // implementations must agree, so the cases are kept identical.
  const cases: Array<[string, string, string, string, string]> = [
    // shared, left, right => L, R
    ["-", "-", "-", "-", "-"],
    ["s", "-", "-", "s", "s"],
    ["-", "l", "-", "l", "-"],
    ["-", "-", "r", "-", "r"],
    ["s", "l", "-", "l", "s"],
    ["s", "-", "r", "s", "r"],
    ["-", "l", "r", "l", "r"],
    ["s", "l", "r", "l", "r"],
  ];

  const roots = (v: string) => (v === "-" ? undefined : [v]);

  it.each(cases)(
    "roots=%s roots_left=%s roots_right=%s -> L=%s R=%s",
    (shared, left, right, expectedL, expectedR) => {
      const out = resolveOverrides({
        roots: roots(shared),
        roots_left: roots(left),
        roots_right: roots(right),
      });
      expect(out.left.roots).toEqual(roots(expectedL));
      expect(out.right.roots).toEqual(roots(expectedR));
    },
  );

  it("applies the same fallback to traversal", () => {
    const out = resolveOverrides({
      traversal: { Key: "tvc_shared" },
      traversal_left: { Inline: {} },
    });
    expect(out.left.traversal).toEqual({ Inline: {} });
    expect(out.right.traversal).toEqual({ Key: "tvc_shared" });
  });

  it("treats an explicit empty root list as an override, not absence", () => {
    const out = resolveOverrides({ roots: ["a"], roots_left: [] });
    expect(out.left.roots).toEqual([]);
    expect(out.right.roots).toEqual(["a"]);
  });
});

describe("parse / serialize", () => {
  it("round-trips every key", () => {
    const params: ExplorerUrlParams = {
      roots: ["a", "b"],
      roots_left: ["c"],
      roots_right: ["d"],
      traversal: { Key: "tvc_1" },
      traversal_left: { Key: "tvc_2" },
      traversal_right: { Key: "tvc_3" },
      graph_settings: "opaque-zstd-base64",
      gqc_delta_left: "opaque-left",
      gqc_delta_right: "opaque-right",
    };

    const raw = serializeExplorerUrlParams(params);
    expect(Object.keys(raw).sort()).toEqual(
      [...EXPLORER_URL_PARAM_KEYS].sort(),
    );
    expect(parseExplorerUrlParams(raw)).toEqual(params);
  });

  it("leaves opaque params unencoded so they stay hand-readable", () => {
    const raw = serializeExplorerUrlParams({ gqc_delta_right: "abc123" });
    expect(raw.gqc_delta_right).toBe("abc123");
  });

  it("JSON-encodes roots", () => {
    const raw = serializeExplorerUrlParams({ roots: ["app", "core"] });
    expect(raw.roots).toBe('["app","core"]');
  });

  it("omits absent and empty values", () => {
    expect(serializeExplorerUrlParams({})).toEqual({});
    expect(serializeExplorerUrlParams({ graph_settings: "" })).toEqual({});
    expect(parseExplorerUrlParams({ roots: "" })).toEqual({});
  });

  // A stale or hand-mangled URL must render the graph with defaults rather
  // than throwing during render.
  it("drops malformed JSON instead of throwing", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(
      parseExplorerUrlParams({ roots: "not json", roots_left: '["ok"]' }),
    ).toEqual({ roots_left: ["ok"] });
    expect(warn).toHaveBeenCalledOnce();
    warn.mockRestore();
  });
});

describe("readExplorerUrlParams", () => {
  it("reads only explorer-owned keys", () => {
    const search = new URLSearchParams(
      'roots=["a"]&gqc_delta_right=xyz&unrelated=keep',
    );
    expect(readExplorerUrlParams(search)).toEqual({
      roots: ["a"],
      gqc_delta_right: "xyz",
    });
  });
});
