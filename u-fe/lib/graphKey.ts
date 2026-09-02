// Copyright (c) Meta Platforms, Inc. and affiliates.

/**
 * Parsing for the canonical `"{timeline}~{graph_id}"` graph key the server
 * returns alongside every graph query.
 *
 * Mirrors `GraphKey::from_str` in `unigraph_core::identifiers`. Timeline IDs may
 * themselves contain `~`, so the split is on the *last* separator.
 */

export interface ParsedGraphKey {
  timeline_id: string;
  graph_id: number;
}

export function parseGraphKey(graphKey: string): ParsedGraphKey {
  const sep = graphKey.lastIndexOf("~");
  if (sep <= 0) {
    throw new Error(
      `Invalid graph key '${graphKey}': expected '<timeline>~<graph_id>'`,
    );
  }

  const timeline_id = graphKey.slice(0, sep);
  const raw_graph_id = graphKey.slice(sep + 1);
  // `Number("")` is 0, so an empty suffix has to be rejected explicitly.
  const graph_id = raw_graph_id === "" ? NaN : Number(raw_graph_id);
  if (!Number.isInteger(graph_id)) {
    throw new Error(
      `Invalid graph key '${graphKey}': graph_id is not an integer`,
    );
  }

  return { timeline_id, graph_id };
}
