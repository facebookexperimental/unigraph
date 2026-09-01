// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Codec and precedence rules for the explorer's URL search params.
 *
 * The shape itself is generated from Rust (`ExplorerUrlParams`), so adding a
 * param there is what adds it here — `EXPLORER_URL_PARAM_KEYS` fails to compile
 * until the new key is listed. This module is the only place that knows how
 * those keys encode and how the two levels of specificity collapse.
 *
 * See `u-be/unigraph_core/src/types/explorer_url_params.rs` for the precedence
 * table; the Rust `resolve()` there is the reference implementation and carries
 * the snapshot test.
 */

import type { ExplorerUrlParams } from "../__generated__/ts/ExplorerUrlParams";
import type { GraphQueryConfig } from "../__generated__/ts/GraphQueryConfig";

/** The overrides a single side can carry, minus the handle that identifies it. */
export type SideOverrides = Pick<GraphQueryConfig, "roots" | "traversal">;

export interface ResolvedOverrides {
  left: SideOverrides;
  right: SideOverrides;
}

/** Wire form: the same keys, values still encoded as strings. */
export type ExplorerUrlParamsRaw = Partial<
  Record<keyof ExplorerUrlParams, string>
>;

/**
 * How each param encodes. `json` values are `JSON.stringify`d; `opaque` ones are
 * already strings on the wire (zstd+base64 or a delta blob) and pass through.
 *
 * Declared as a `Record` over the generated type on purpose: adding a field in
 * Rust makes this object a compile error until the new key is classified, which
 * is what keeps the codec and the type from drifting.
 */
const PARAM_ENCODING: Record<keyof ExplorerUrlParams, "json" | "opaque"> = {
  roots: "json",
  roots_left: "json",
  roots_right: "json",
  traversal: "json",
  traversal_left: "json",
  traversal_right: "json",
  graph_settings: "opaque",
  gqc_delta_left: "opaque",
  gqc_delta_right: "opaque",
};

/**
 * Every key the explorer owns in the query string.
 *
 * Consumers that manage the URL themselves use this to tell explorer-owned
 * params from their own, rather than assuming they own the whole query string.
 */
export const EXPLORER_URL_PARAM_KEYS = Object.keys(
  PARAM_ENCODING,
) as (keyof ExplorerUrlParams)[];

/**
 * Decode the query string into the typed shape.
 *
 * A param that fails to parse is dropped rather than thrown: a hand-edited or
 * stale URL should render the graph with defaults, not an error page.
 */
export function parseExplorerUrlParams(
  raw: ExplorerUrlParamsRaw,
): ExplorerUrlParams {
  const out: ExplorerUrlParams = {};
  for (const key of EXPLORER_URL_PARAM_KEYS) {
    const value = raw[key];
    if (value == null || value === "") continue;
    if (PARAM_ENCODING[key] === "json") {
      const parsed = parseJson(value, key);
      if (parsed !== undefined) {
        // Each field has its own value type, so the write can only be checked
        // one key at a time — the key itself is still typed.
        (out as Record<string, unknown>)[key] = parsed;
      }
    } else {
      (out as Record<string, unknown>)[key] = value;
    }
  }
  return out;
}

/** Encode back to the wire form. Absent and empty values are omitted. */
export function serializeExplorerUrlParams(
  params: ExplorerUrlParams,
): ExplorerUrlParamsRaw {
  const out: ExplorerUrlParamsRaw = {};
  for (const key of EXPLORER_URL_PARAM_KEYS) {
    const value = params[key];
    if (value == null) continue;
    if (PARAM_ENCODING[key] === "json") {
      out[key] = JSON.stringify(value);
    } else if (value !== "") {
      out[key] = value as string;
    }
  }
  return out;
}

/**
 * Collapse the bare/`_left`/`_right` keys into what each side should use.
 *
 * A side-specific key wins for that side; otherwise the bare key applies to
 * both. `left` is meaningful only in delta view — a single-graph caller reads
 * `right` and ignores `left`.
 */
export function resolveOverrides(params: ExplorerUrlParams): ResolvedOverrides {
  return {
    left: {
      roots: pick(params.roots_left, params.roots),
      traversal: pick(params.traversal_left, params.traversal),
    },
    right: {
      roots: pick(params.roots_right, params.roots),
      traversal: pick(params.traversal_right, params.traversal),
    },
  };
}

/** Read the explorer-owned params out of a `URLSearchParams`. */
export function readExplorerUrlParams(
  search: URLSearchParams,
): ExplorerUrlParams {
  const raw: ExplorerUrlParamsRaw = {};
  for (const key of EXPLORER_URL_PARAM_KEYS) {
    const value = search.get(key);
    if (value != null) raw[key] = value;
  }
  return parseExplorerUrlParams(raw);
}

// --- Internals ---------------------------------------------------------------

/** The whole fallback rule: a side-specific value wins, otherwise the shared one. */
function pick<T>(side: T | undefined, shared: T | undefined): T | undefined {
  return side !== undefined ? side : shared;
}

function parseJson(value: string, key: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    console.warn(`Ignoring malformed URL param "${key}": ${value}`);
    return undefined;
  }
}
