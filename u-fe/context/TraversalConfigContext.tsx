// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";
import type { Arrow } from "@/__generated__/ts/Arrow";
import type { TraversalConfig } from "@/__generated__/ts/TraversalConfig";
import {
  ARROW_POINTS_FROM_NON_EXISTENT,
  useCanEdgeBeForcedR,
  useCanNodeBeForceExcludedR,
} from "../ArrowUtils";
import { useNativeGraphR } from "./NativeGraphContext";

/// Edits staged in the config editor but not yet pushed to the graph.
///
/// Committing a config is expensive — it re-serializes the whole thing and
/// re-runs the traversal — so the editor batches edits here and the user
/// decides when to pay for them. See [`TraversalConfigEditorPanel`].
export type TvcDraft = {
  /// `null` when nothing is staged.
  draft: TraversalConfig | null;
  setDraft: (tvc: TraversalConfig) => void;
  apply: () => void;
  discard: () => void;
  /// The committed config changed underneath the draft — a context menu or
  /// keyboard shortcut writes straight through — so applying now would revert
  /// that edit. The editor surfaces this instead of silently clobbering it.
  isStale: boolean;
};

export type TraversalConfigContextType = {
  tvcL: TraversalConfig | null;
  setTvcL: (tvc: TraversalConfig) => void;
  tvcR: TraversalConfig;
  setTvcR: (tvc: TraversalConfig) => void;
  /// Drafts live here rather than in the panel because switching sidebar tabs
  /// unmounts the panel, which would otherwise drop staged edits.
  draftL: TvcDraft;
  draftR: TvcDraft;
};

const TraversalConfigContext = createContext<TraversalConfigContextType | null>(
  null,
);

export function TraversalConfigContextProvider({
  children,
  tvcL,
  setTvcL,
  tvcR,
  setTvcR,
}: {
  children: React.ReactNode;
  tvcL: TraversalConfig | null;
  setTvcL: (tvc: TraversalConfig) => void;
  tvcR: TraversalConfig;
  setTvcR: (tvc: TraversalConfig) => void;
}) {
  const draftL = useTvcDraft(tvcL, setTvcL);
  const draftR = useTvcDraft(tvcR, setTvcR);

  const value = useMemo(
    () => ({ tvcL, setTvcL, tvcR, setTvcR, draftL, draftR }),
    [tvcL, setTvcL, tvcR, setTvcR, draftL, draftR],
  );
  return (
    <TraversalConfigContext.Provider value={value}>
      {children}
    </TraversalConfigContext.Provider>
  );
}

function useTvcDraft(
  committed: TraversalConfig | null,
  setCommitted: (tvc: TraversalConfig) => void,
): TvcDraft {
  // `base` is the committed config the draft was branched from, kept so we can
  // tell whether anything else has written since.
  const [staged, setStaged] = useState<{
    draft: TraversalConfig;
    base: TraversalConfig | null;
  } | null>(null);

  const setDraft = useCallback(
    (tvc: TraversalConfig) => {
      setStaged((prev) => ({ draft: tvc, base: prev?.base ?? committed }));
    },
    [committed],
  );

  const apply = useCallback(() => {
    setStaged((prev) => {
      if (prev != null) {
        setCommitted(prev.draft);
      }
      return null;
    });
  }, [setCommitted]);

  const discard = useCallback(() => setStaged(null), []);

  return useMemo(
    () => ({
      draft: staged?.draft ?? null,
      setDraft,
      apply,
      discard,
      isStale: staged != null && staged.base !== committed,
    }),
    [staged, setDraft, apply, discard, committed],
  );
}

/// Number of individual entries that differ between two configs.
///
/// Relies on edits being immutable copies: an untouched subtree keeps its
/// identity, so reference equality prunes whole branches and this stays cheap
/// even on configs with hundreds of thousands of entries.
export function countTvcChanges(
  draft: TraversalConfig,
  committed: TraversalConfig,
): number {
  return (
    countMap(draft.force_nodes, committed.force_nodes) +
    countNestedMap(draft.force_edges, committed.force_edges) +
    countMap(draft.force_tagged, committed.force_tagged) +
    countMap(draft.label_predicates, committed.label_predicates) +
    countDynamic(draft.force_dynamic, committed.force_dynamic) +
    countMap(draft.messages, committed.messages) +
    (draft.tiered_traversal !== committed.tiered_traversal ? 1 : 0)
  );
}

function countMap<T>(
  a: { [key: string]: T } | undefined,
  b: { [key: string]: T } | undefined,
): number {
  if (a === b) return 0;
  let count = 0;
  for (const key of new Set([
    ...Object.keys(a ?? {}),
    ...Object.keys(b ?? {}),
  ])) {
    if (a?.[key] !== b?.[key]) count++;
  }
  return count;
}

function countNestedMap<T>(
  a: { [key: string]: { [key: string]: T } } | undefined,
  b: { [key: string]: { [key: string]: T } } | undefined,
): number {
  if (a === b) return 0;
  let count = 0;
  for (const key of new Set([
    ...Object.keys(a ?? {}),
    ...Object.keys(b ?? {}),
  ])) {
    count += countMap(a?.[key], b?.[key]);
  }
  return count;
}

function countDynamic(
  a: TraversalConfig["force_dynamic"],
  b: TraversalConfig["force_dynamic"],
): number {
  if (a === b) return 0;
  let count = 0;
  for (const key of new Set([
    ...Object.keys(a ?? {}),
    ...Object.keys(b ?? {}),
  ])) {
    const left = a?.[key];
    const right = b?.[key];
    if (left === right) continue;
    if (left == null || right == null) {
      count++;
      continue;
    }
    if (left.default_branches !== right.default_branches) count++;
    count += countMap(left.overrides, right.overrides);
  }
  return count;
}

export function useTVC(): TraversalConfigContextType {
  const context = useContext(TraversalConfigContext);

  if (context == null) {
    throw new Error("useTVC must be used within a TraversalConfigProvider");
  }
  return context;
}

export function useFlipForceEdgeL(arrow: Arrow | null): {
  enabled: boolean;
  forceEdge: () => void;
  action: "Include" | "Exclude";
} {
  const { tvcR: tvc, setTvcR: setTvc } = useTVC();
  const nativeGraph = useNativeGraphR();

  const pointsTo = arrow?.points_to ?? null;
  const pointsFrom = arrow?.points_from ?? null;

  const fromName =
    pointsFrom != null && pointsFrom !== ARROW_POINTS_FROM_NON_EXISTENT
      ? nativeGraph.getNodeName(pointsFrom)
      : null;
  const toName = pointsTo != null ? nativeGraph.getNodeName(pointsTo) : null;

  const enabled = useCanEdgeBeForcedR(arrow);

  // true/false if forced. null if there is no force edge/not set
  const isForcedTo =
    fromName != null && toName != null
      ? (tvc.force_edges?.[fromName]?.[toName]?.include ?? null)
      : null;

  const action: "Include" | "Exclude" = (() => {
    if (isForcedTo === null) {
      return arrow?.excluded ? "Include" : "Exclude";
    }
    return isForcedTo ? "Exclude" : "Include";
  })();

  const forceEdge = useCallback(() => {
    if (enabled === false || fromName == null || toName == null) {
      return;
    }

    setTvc({
      ...tvc,
      force_edges: {
        ...tvc.force_edges,
        [fromName]: {
          ...tvc.force_edges?.[fromName],
          [toName]: { include: action === "Include", message_id: undefined },
        },
      },
    });
  }, [tvc, setTvc, fromName, toName, action, enabled]);

  return useMemo(() => {
    return { action, enabled, forceEdge };
  }, [action, enabled, forceEdge]);
}

export function useFlipForceExcludeNodeL(arrow: Arrow | null): {
  enabled: boolean;
  action: "Include" | "Exclude";
  forceExcludeNode: () => void;
} {
  const { tvcR: tvc, setTvcR: setTvc } = useTVC();
  const nativeGraph = useNativeGraphR();
  const enabled = useCanNodeBeForceExcludedR(arrow);

  const pointsTo = arrow?.points_to ?? null;
  const nodeName = pointsTo != null ? nativeGraph.getNodeName(pointsTo) : null;

  const action =
    nodeName != null
      ? tvc.force_nodes?.[nodeName]?.include === false
        ? "Include"
        : "Exclude"
      : "Include";

  const forceExcludeNode = useCallback(() => {
    if (enabled === false || nodeName == null) {
      return;
    }

    if (action === "Exclude") {
      setTvc({
        ...tvc,
        force_nodes: {
          ...tvc.force_nodes,
          [nodeName]: { include: false, message_id: undefined },
        },
      });
    } else {
      const { [nodeName]: _, ...rest } = tvc.force_nodes ?? {};
      setTvc({
        ...tvc,
        force_nodes: rest,
      });
    }
  }, [tvc, setTvc, enabled, action, nodeName]);

  return useMemo(() => {
    return {
      enabled,
      action,
      forceExcludeNode,
    };
  }, [enabled, action, forceExcludeNode]);
}
