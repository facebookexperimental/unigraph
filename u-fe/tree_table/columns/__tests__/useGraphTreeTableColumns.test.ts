// Copyright (c) Meta Platforms, Inc. and affiliates.

import { describe, expect, it } from "vitest";
import type { ArrayGraphStats } from "../../../__generated__/ts/ArrayGraphStats";
import type { GraphSettings } from "../../../__generated__/ts/GraphSettings";
import type { TraversalConfig } from "../../../__generated__/ts/TraversalConfig";
import type NativeGraph from "../../../native/NativeGraph";
import type TwinGraph from "../../../native/TwinGraph";
import {
  ColumnsCtx,
  DeltaGraphColumnsBuilder,
  SingleGraphColumnsBuilder,
} from "../useGraphTreeTableColumns";

// ── Stubs ──────────────────────────────────────────────────────
// Builder shape logic (which columns get created) never touches WASM:
// `makeColumns()`/`definition()` only read `metricNames` and `stats()`.
// The lazy `renderer`/`getNumericValues` closures (which do hit WASM) are
// never invoked here.

const STATS: ArrayGraphStats = {
  num_all_nodes: 0,
  num_all_edges: 0,
  num_directed_edges: 0,
  num_tagged_edges: 0,
  num_dynamic_edges: 0,
  num_unreachable_nodes: 0,
  num_excluded_edges: 0,
  tier_names: ["T1", "T2"],
};

function stubGraph(metricNames: string[]): NativeGraph {
  return {
    metricNames,
    stats: () => STATS,
  } as unknown as NativeGraph;
}

function stubTwinGraph(hasLeft: boolean, metricNames: string[]): TwinGraph {
  const r = stubGraph(metricNames);
  const l = hasLeft ? stubGraph(metricNames) : null;
  return {
    l,
    r,
    leftGraphX: () => l,
  } as unknown as TwinGraph;
}

// `node_type` is an enum metric (numeric value + Enum format); `size` is a
// plain numeric metric that should still get the full transitive/tiered set.
const GRAPH_SETTINGS: GraphSettings = {
  metrics_config: {
    metrics: {
      node_type: { format: { Enum: { variants: { 0: "root", 1: "nested" } } } },
      size: {},
    },
  },
};

const TVC: TraversalConfig = {};

function ctx(
  settings: GraphSettings = GRAPH_SETTINGS,
  isDelta = false,
): ColumnsCtx {
  return new ColumnsCtx(settings, () => {}, TVC, new Set<string>(), isDelta);
}

function withStoredSort(key: string): GraphSettings {
  return {
    ...GRAPH_SETTINGS,
    ui_settings: {
      columns: {
        graph_table_sort: {
          column: { MetricView: { key } },
          order: "Desc",
        },
      },
    },
  };
}

function columnIDsFor(
  builder: SingleGraphColumnsBuilder | DeltaGraphColumnsBuilder,
): string[] {
  return builder.makeColumns().map((c) => c.definition()[0]);
}

// ── Tests ──────────────────────────────────────────────────────

describe("enum metrics collapse to a single column", () => {
  it("single graph: enum metric produces exactly one column, no aggregations", () => {
    const twin = stubTwinGraph(false, ["node_type", "size"]);
    const ids = columnIDsFor(
      new SingleGraphColumnsBuilder(
        twin,
        GRAPH_SETTINGS,
        () => {},
        TVC,
        new Set<string>(),
      ),
    );

    const enumIDs = ids.filter((id) => id.includes("node_type"));
    expect(enumIDs).toEqual(["node_type"]);

    // The plain numeric metric still gets the full explosion.
    expect(ids).toContain("T(size)");
    expect(ids).toContain("D(size)");
    expect(ids).toContain("T1 size");
  });

  it("delta graph: enum metric produces exactly one column, no aggregations", () => {
    const twin = stubTwinGraph(true, ["node_type", "size"]);
    const ids = columnIDsFor(
      new DeltaGraphColumnsBuilder(
        twin,
        GRAPH_SETTINGS,
        () => {},
        TVC,
        new Set<string>(),
      ),
    );

    const enumIDs = ids.filter((id) => id.includes("node_type"));
    expect(enumIDs).toEqual(["node_type"]);

    // The plain numeric metric still gets its delta columns.
    expect(ids).toContain("∆(size)");
    expect(ids).toContain("∆T(size)");
  });
});

describe("ColumnsCtx.isEnum", () => {
  it("detects enum-formatted metrics", () => {
    expect(ctx().isEnum("node_type")).toBe(true);
    expect(ctx().isEnum("size")).toBe(false);
    expect(ctx().isEnum("nonexistent")).toBe(false);
  });
});

/// A graph stores one sort preference and it has to mean something in both
/// views. `ColumnsCtx.sort()` is the single place every column reads it, so
/// this is where the per-mode rule has to hold.
///
/// Mirrors `sort_key_resolves_per_mode` in
/// `u-be/unigraph_core/src/types/array_graph/graph_settings.rs`.
describe("stored sort resolves per mode", () => {
  function resolved(key: string, isDelta: boolean): string | null {
    const sort = ctx(withStoredSort(key), isDelta).sort();
    if (sort == null || !("MetricView" in sort.column)) return null;
    return sort.column.MetricView.key;
  }

  it("drops the side outside delta mode, and never adds one", () => {
    const keys = [
      "size",
      "size~transitive",
      "size#T2",
      "size#T2~dominated",
      "node-count~transitive",
      "size~transitive@left",
      "size~transitive@delta",
      "size#T2@delta",
      "node-count~transitive@delta",
    ];

    const table = keys
      .map(
        (key) =>
          `${key.padEnd(30)} ${String(resolved(key, true)).padEnd(28)} ${resolved(key, false)}`,
      )
      .join("\n");

    expect(
      `${"stored".padEnd(30)} ${"delta view".padEnd(28)} single graph\n${table}`,
    ).toMatchInlineSnapshot(`
        "stored                         delta view                   single graph
        size                           size                         size
        size~transitive                size~transitive              size~transitive
        size#T2                        size#T2                      size#T2
        size#T2~dominated              size#T2~dominated            size#T2~dominated
        node-count~transitive          node-count~transitive        node-count~transitive
        size~transitive@left           size~transitive@left         size~transitive
        size~transitive@delta          size~transitive@delta        size~transitive
        size#T2@delta                  size#T2@delta                size#T2
        node-count~transitive@delta    node-count~transitive@delta  node-count~transitive"
      `);
  });

  it("carries the order through untouched", () => {
    expect(ctx(withStoredSort("size#T2@delta"), false).sort()?.order).toBe(
      "Desc",
    );
  });

  it("leaves a NodeName sort alone", () => {
    const settings: GraphSettings = {
      ...GRAPH_SETTINGS,
      ui_settings: {
        columns: {
          graph_table_sort: { column: { NodeName: {} }, order: "Asc" },
        },
      },
    };
    expect(ctx(settings, false).sort()).toEqual({
      column: { NodeName: {} },
      order: "Asc",
    });
  });

  it("is null when the graph stores nothing", () => {
    expect(ctx(GRAPH_SETTINGS, true).sort()).toBeNull();
  });
});
