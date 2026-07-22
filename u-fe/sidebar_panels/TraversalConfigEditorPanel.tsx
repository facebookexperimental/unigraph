// Copyright (c) Meta Platforms, Inc. and affiliates.

import { ChevronRight, Plus, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import type { Decision } from "../__generated__/ts/Decision";
import type { DynamicTypeConfig } from "../__generated__/ts/DynamicTypeConfig";
import type { NodeLabelPredicate } from "../__generated__/ts/NodeLabelPredicate";
import type { TieredTraversalConfig } from "../__generated__/ts/TieredTraversalConfig";
import type { TraversalConfig } from "../__generated__/ts/TraversalConfig";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "../components/ui/collapsible";
import { Input } from "../components/ui/input";
import NodeNameInput from "../components/NodeNameInput";
import { Label } from "../components/ui/label";
import { Separator } from "../components/ui/separator";
import { Switch } from "../components/ui/switch";
import { useNativeGraphs } from "../context/NativeGraphContext";
import { useTVC } from "../context/TraversalConfigContext";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function TraversalConfigEditorPanel() {
  const [nativeGraphL] = useNativeGraphs();
  const { tvcL, setTvcL, tvcR, setTvcR } = useTVC();

  const labelR = nativeGraphL == null ? "" : " (Right)";
  const labelL = " (Left)";

  return (
    <SidebarPanel storageKey="traversal-config">
      <div className="flex flex-col gap-6">
        <TraversalConfigEditor tvc={tvcR} setTvc={setTvcR} label={labelR} />
        {nativeGraphL != null && tvcL != null && (
          <TraversalConfigEditor tvc={tvcL} setTvc={setTvcL} label={labelL} />
        )}
      </div>
    </SidebarPanel>
  );
}

function TraversalConfigEditor({
  tvc,
  setTvc,
  label,
}: {
  tvc: TraversalConfig;
  setTvc: (tvc: TraversalConfig) => void;
  label: string;
}) {
  return (
    <div className="flex flex-col gap-2">
      <SidebarPanelHeader text={`Traversal Config${label}`} />
      <ForceNodesEditor tvc={tvc} setTvc={setTvc} />
      <ForceEdgesEditor tvc={tvc} setTvc={setTvc} />
      <ForceTaggedEditor tvc={tvc} setTvc={setTvc} />
      <LabelPredicatesEditor tvc={tvc} setTvc={setTvc} />
      <ForceDynamicEditor tvc={tvc} setTvc={setTvc} />
      <TieredTraversalEditor tvc={tvc} setTvc={setTvc} />
      <MessagesEditor tvc={tvc} setTvc={setTvc} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

type TvcEditorProps = {
  tvc: TraversalConfig;
  setTvc: (tvc: TraversalConfig) => void;
};

function Section({
  title,
  count,
  children,
}: {
  title: string;
  count: number;
  children: React.ReactNode;
}) {
  return (
    <Collapsible>
      <CollapsibleTrigger className="flex items-center gap-2 w-full cursor-pointer group">
        <ChevronRight className="size-4 transition-transform group-data-[state=open]:rotate-90" />
        <span className="text-sm font-medium">{title}</span>
        <Badge variant="secondary" className="text-xs">
          {count}
        </Badge>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-2 pt-2 pl-6">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function EntryRow({
  onRemove,
  children,
}: {
  onRemove: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-2 text-sm">
      <div className="flex items-center gap-2 flex-1 min-w-0">{children}</div>
      <Button
        size="icon"
        variant="ghost"
        className="cursor-pointer size-7 shrink-0"
        onClick={onRemove}
      >
        <Trash2 className="size-3" />
      </Button>
    </div>
  );
}

function IncludeSwitch({
  include,
  onChange,
}: {
  include: boolean;
  onChange: (include: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-1.5 shrink-0">
      <Switch checked={include} onCheckedChange={onChange} />
      <span className="text-xs text-muted-foreground w-12">
        {include ? "Include" : "Exclude"}
      </span>
    </div>
  );
}

function AddButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      size="sm"
      variant="ghost"
      className="cursor-pointer self-start"
      onClick={onClick}
    >
      <Plus className="size-3" />
      Add
    </Button>
  );
}

// ---------------------------------------------------------------------------
// ForceNodesEditor
// ---------------------------------------------------------------------------

function ForceNodesEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries = Object.entries(tvc.force_nodes ?? {});
  const [newName, setNewName] = useState("");

  const update = useCallback(
    (force_nodes: TraversalConfig["force_nodes"]) => {
      setTvc({ ...tvc, force_nodes });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const name = newName.trim();
    if (name === "") return;
    update({
      ...tvc.force_nodes,
      [name]: { include: false },
    });
    setNewName("");
  }, [newName, tvc.force_nodes, update]);

  const remove = useCallback(
    (name: string) => {
      const { [name]: _, ...rest } = tvc.force_nodes ?? {};
      update(Object.keys(rest).length > 0 ? rest : undefined);
    },
    [tvc.force_nodes, update],
  );

  const toggle = useCallback(
    (name: string, include: boolean) => {
      update({
        ...tvc.force_nodes,
        [name]: { ...tvc.force_nodes?.[name], include },
      });
    },
    [tvc.force_nodes, update],
  );

  return (
    <Section title="Force Nodes" count={entries.length}>
      {entries.map(([name, decision]) => (
        <EntryRow key={name} onRemove={() => remove(name)}>
          <span className="truncate" title={name}>
            {name}
          </span>
          <IncludeSwitch
            include={decision.include}
            onChange={(v) => toggle(name, v)}
          />
        </EntryRow>
      ))}
      <div className="flex items-center gap-2">
        <NodeNameInput
          value={newName}
          onChange={setNewName}
          onSelect={(name) => {
            update({
              ...tvc.force_nodes,
              [name]: { include: false },
            });
            setNewName("");
          }}
        />
        <AddButton onClick={add} />
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// ForceEdgesEditor
// ---------------------------------------------------------------------------

function ForceEdgesEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries: [string, string, Decision][] = [];
  for (const [from, tos] of Object.entries(tvc.force_edges ?? {})) {
    for (const [to, decision] of Object.entries(tos)) {
      entries.push([from, to, decision]);
    }
  }

  const [newFrom, setNewFrom] = useState("");
  const [newTo, setNewTo] = useState("");

  const update = useCallback(
    (force_edges: TraversalConfig["force_edges"]) => {
      setTvc({ ...tvc, force_edges });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const from = newFrom.trim();
    const to = newTo.trim();
    if (from === "" || to === "") return;
    update({
      ...tvc.force_edges,
      [from]: {
        ...tvc.force_edges?.[from],
        [to]: { include: false },
      },
    });
    setNewFrom("");
    setNewTo("");
  }, [newFrom, newTo, tvc.force_edges, update]);

  const remove = useCallback(
    (from: string, to: string) => {
      const edges = { ...tvc.force_edges };
      if (edges[from]) {
        const { [to]: _, ...rest } = edges[from];
        if (Object.keys(rest).length > 0) {
          edges[from] = rest;
        } else {
          delete edges[from];
        }
      }
      update(Object.keys(edges).length > 0 ? edges : undefined);
    },
    [tvc.force_edges, update],
  );

  const toggle = useCallback(
    (from: string, to: string, include: boolean) => {
      update({
        ...tvc.force_edges,
        [from]: {
          ...tvc.force_edges?.[from],
          [to]: { ...tvc.force_edges?.[from]?.[to], include },
        },
      });
    },
    [tvc.force_edges, update],
  );

  return (
    <Section title="Force Edges" count={entries.length}>
      {entries.map(([from, to, decision]) => (
        <EntryRow key={`${from}->${to}`} onRemove={() => remove(from, to)}>
          <span className="truncate" title={`${from} → ${to}`}>
            {from} → {to}
          </span>
          <IncludeSwitch
            include={decision.include}
            onChange={(v) => toggle(from, to, v)}
          />
        </EntryRow>
      ))}
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <NodeNameInput
            value={newFrom}
            onChange={setNewFrom}
            onSelect={setNewFrom}
            placeholder="From node"
          />
          <span className="text-xs text-muted-foreground">→</span>
          <NodeNameInput
            value={newTo}
            onChange={setNewTo}
            onSelect={(name) => {
              setNewTo(name);
              if (newFrom.trim() !== "") {
                update({
                  ...tvc.force_edges,
                  [newFrom.trim()]: {
                    ...tvc.force_edges?.[newFrom.trim()],
                    [name]: { include: false },
                  },
                });
                setNewFrom("");
                setNewTo("");
              }
            }}
            placeholder="To node"
          />
          <AddButton onClick={add} />
        </div>
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// ForceTaggedEditor
// ---------------------------------------------------------------------------

function ForceTaggedEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries = Object.entries(tvc.force_tagged ?? {});
  const [newTag, setNewTag] = useState("");

  const update = useCallback(
    (force_tagged: TraversalConfig["force_tagged"]) => {
      setTvc({ ...tvc, force_tagged });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const tag = newTag.trim();
    if (tag === "") return;
    update({
      ...tvc.force_tagged,
      [tag]: { include: false },
    });
    setNewTag("");
  }, [newTag, tvc.force_tagged, update]);

  const remove = useCallback(
    (tag: string) => {
      const { [tag]: _, ...rest } = tvc.force_tagged ?? {};
      update(Object.keys(rest).length > 0 ? rest : undefined);
    },
    [tvc.force_tagged, update],
  );

  const toggle = useCallback(
    (tag: string, include: boolean) => {
      update({
        ...tvc.force_tagged,
        [tag]: { ...tvc.force_tagged?.[tag], include },
      });
    },
    [tvc.force_tagged, update],
  );

  return (
    <Section title="Force Tagged" count={entries.length}>
      {entries.map(([tag, decision]) => (
        <EntryRow key={tag} onRemove={() => remove(tag)}>
          <Badge variant="outline" className="text-xs">
            {tag}
          </Badge>
          <IncludeSwitch
            include={decision.include}
            onChange={(v) => toggle(tag, v)}
          />
        </EntryRow>
      ))}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Tag name"
          value={newTag}
          onChange={(e) => setNewTag(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          className="h-7 text-xs"
        />
        <AddButton onClick={add} />
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// LabelPredicatesEditor
// ---------------------------------------------------------------------------

function LabelPredicatesEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries = Object.entries(tvc.label_predicates ?? {});
  const [newKey, setNewKey] = useState("");

  const update = useCallback(
    (label_predicates: TraversalConfig["label_predicates"]) => {
      setTvc({ ...tvc, label_predicates });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const key = newKey.trim();
    if (key === "") return;
    update({
      ...tvc.label_predicates,
      [key]: {
        label_name: "",
        label_value: "",
        contains: true,
        decision: { include: true },
      },
    });
    setNewKey("");
  }, [newKey, tvc.label_predicates, update]);

  const remove = useCallback(
    (key: string) => {
      const { [key]: _, ...rest } = tvc.label_predicates ?? {};
      update(Object.keys(rest).length > 0 ? rest : undefined);
    },
    [tvc.label_predicates, update],
  );

  const updatePredicate = useCallback(
    (key: string, patch: Partial<NodeLabelPredicate>) => {
      const current = tvc.label_predicates?.[key];
      if (current == null) return;
      update({
        ...tvc.label_predicates,
        [key]: { ...current, ...patch },
      });
    },
    [tvc.label_predicates, update],
  );

  return (
    <Section title="Label Predicates" count={entries.length}>
      {entries.map(([key, pred]) => (
        <div key={key} className="flex flex-col gap-1.5 pb-2">
          <EntryRow onRemove={() => remove(key)}>
            <span className="font-medium text-xs">{key}</span>
          </EntryRow>
          <div className="pl-2 flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <Label className="text-xs w-16 shrink-0">Name</Label>
              <Input
                value={pred.label_name}
                onChange={(e) =>
                  updatePredicate(key, { label_name: e.target.value })
                }
                className="h-7 text-xs"
              />
            </div>
            <div className="flex items-center gap-2">
              <Label className="text-xs w-16 shrink-0">Value</Label>
              <Input
                value={pred.label_value}
                onChange={(e) =>
                  updatePredicate(key, { label_value: e.target.value })
                }
                className="h-7 text-xs"
              />
            </div>
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-1.5">
                <Switch
                  checked={pred.contains}
                  onCheckedChange={(v) => updatePredicate(key, { contains: v })}
                />
                <span className="text-xs text-muted-foreground">
                  {pred.contains ? "Contains" : "Not contains"}
                </span>
              </div>
              <IncludeSwitch
                include={pred.decision.include}
                onChange={(include) =>
                  updatePredicate(key, {
                    decision: { ...pred.decision, include },
                  })
                }
              />
            </div>
          </div>
          <Separator />
        </div>
      ))}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Predicate key"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          className="h-7 text-xs"
        />
        <AddButton onClick={add} />
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// ForceDynamicEditor
// ---------------------------------------------------------------------------

function ForceDynamicEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries = Object.entries(tvc.force_dynamic ?? {});
  const [newKey, setNewKey] = useState("");

  const update = useCallback(
    (force_dynamic: TraversalConfig["force_dynamic"]) => {
      setTvc({ ...tvc, force_dynamic });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const key = newKey.trim();
    if (key === "") return;
    update({
      ...tvc.force_dynamic,
      [key]: {},
    });
    setNewKey("");
  }, [newKey, tvc.force_dynamic, update]);

  const remove = useCallback(
    (key: string) => {
      const { [key]: _, ...rest } = tvc.force_dynamic ?? {};
      update(Object.keys(rest).length > 0 ? rest : undefined);
    },
    [tvc.force_dynamic, update],
  );

  const updateConfig = useCallback(
    (key: string, patch: Partial<DynamicTypeConfig>) => {
      const current = tvc.force_dynamic?.[key];
      if (current == null) return;
      update({
        ...tvc.force_dynamic,
        [key]: { ...current, ...patch },
      });
    },
    [tvc.force_dynamic, update],
  );

  return (
    <Section title="Force Dynamic" count={entries.length}>
      {entries.map(([key, config]) => (
        <div key={key} className="flex flex-col gap-1.5 pb-2">
          <EntryRow onRemove={() => remove(key)}>
            <span className="font-medium text-xs">{key}</span>
          </EntryRow>
          <div className="pl-2 flex flex-col gap-1">
            <DynamicBranchesEditor
              config={config}
              onChange={(patch) => updateConfig(key, patch)}
            />
            {config.overrides != null &&
              Object.keys(config.overrides).length > 0 && (
                <div className="text-xs text-muted-foreground">
                  {Object.keys(config.overrides).length} override(s)
                </div>
              )}
          </div>
          <Separator />
        </div>
      ))}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Dynamic type key"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          className="h-7 text-xs"
        />
        <AddButton onClick={add} />
      </div>
    </Section>
  );
}

function DynamicBranchesEditor({
  config,
  onChange,
}: {
  config: DynamicTypeConfig;
  onChange: (patch: Partial<DynamicTypeConfig>) => void;
}) {
  const branches = config.default_branches;
  const isInclude = branches != null && "Include" in branches;
  const branchList =
    branches != null
      ? isInclude
        ? branches.Include
        : (branches as { Exclude: string[] }).Exclude
      : [];
  const [newBranch, setNewBranch] = useState("");

  const setBranches = useCallback(
    (variant: "Include" | "Exclude", list: string[]) => {
      onChange({
        default_branches:
          list.length > 0
            ? ({ [variant]: list } as DynamicTypeConfig["default_branches"])
            : undefined,
      });
    },
    [onChange],
  );

  const variant = isInclude ? "Include" : "Exclude";

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <Label className="text-xs">Default branches</Label>
        <Button
          size="sm"
          variant="ghost"
          className="cursor-pointer text-xs h-6 px-2"
          onClick={() => {
            const next = isInclude ? "Exclude" : "Include";
            setBranches(next, branchList);
          }}
        >
          {variant}
        </Button>
      </div>
      {branchList.map((branch, i) => (
        <div key={i} className="flex items-center gap-1">
          <Badge variant="outline" className="text-xs">
            {branch}
          </Badge>
          <Button
            size="icon"
            variant="ghost"
            className="cursor-pointer size-5"
            onClick={() => {
              const next = branchList.filter((_, idx) => idx !== i);
              setBranches(variant, next);
            }}
          >
            <Trash2 className="size-3" />
          </Button>
        </div>
      ))}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Branch name"
          value={newBranch}
          onChange={(e) => setNewBranch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              const name = newBranch.trim();
              if (name === "") return;
              setBranches(variant, [...branchList, name]);
              setNewBranch("");
            }
          }}
          className="h-6 text-xs"
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// TieredTraversalEditor
// ---------------------------------------------------------------------------

function TieredTraversalEditor({ tvc, setTvc }: TvcEditorProps) {
  const tiered = tvc.tiered_traversal;
  const ascending = tiered?.AscendingTiers;
  const tiers = ascending?.tiers ?? [];
  const maxTier = ascending?.max_tier;

  const [newTierName, setNewTierName] = useState("");

  const updateTiered = useCallback(
    (tiered_traversal: TieredTraversalConfig | undefined) => {
      setTvc({ ...tvc, tiered_traversal });
    },
    [tvc, setTvc],
  );

  const addTier = useCallback(() => {
    const name = newTierName.trim();
    if (name === "") return;
    updateTiered({
      AscendingTiers: {
        tiers: [
          ...tiers,
          {
            name,
            tags_that_transition_to_this_tier: [],
            dynamic_type_keys_that_transition_to_this_tier: [],
          },
        ],
        max_tier: maxTier,
      },
    });
    setNewTierName("");
  }, [newTierName, tiers, maxTier, updateTiered]);

  const removeTier = useCallback(
    (index: number) => {
      const next = tiers.filter((_, i) => i !== index);
      if (next.length === 0) {
        updateTiered(undefined);
      } else {
        updateTiered({
          AscendingTiers: {
            tiers: next,
            max_tier:
              maxTier != null && maxTier >= next.length
                ? next.length - 1
                : maxTier,
          },
        });
      }
    },
    [tiers, maxTier, updateTiered],
  );

  const updateMaxTier = useCallback(
    (value: string) => {
      const parsed = value === "" ? undefined : Number.parseInt(value, 10);
      updateTiered({
        AscendingTiers: {
          tiers,
          max_tier:
            parsed != null && !Number.isNaN(parsed) ? parsed : undefined,
        },
      });
    },
    [tiers, updateTiered],
  );

  return (
    <Section title="Tiered Traversal" count={tiers.length}>
      {tiers.map((tier, i) => (
        <div key={i} className="flex flex-col gap-1 pb-2">
          <EntryRow onRemove={() => removeTier(i)}>
            <span className="font-medium text-xs">
              Tier {i}: {tier.name}
            </span>
          </EntryRow>
          <div className="pl-2 flex flex-wrap gap-1">
            {tier.tags_that_transition_to_this_tier.map((tag) => (
              <Badge key={tag} variant="outline" className="text-xs">
                {tag}
              </Badge>
            ))}
            {tier.tags_that_transition_to_this_tier.length === 0 && (
              <span className="text-xs text-muted-foreground">No tags</span>
            )}
          </div>
        </div>
      ))}
      {tiers.length > 0 && (
        <div className="flex items-center gap-2">
          <Label className="text-xs shrink-0">Max tier</Label>
          <Input
            type="number"
            value={maxTier ?? ""}
            onChange={(e) => updateMaxTier(e.target.value)}
            placeholder="None"
            className="h-7 text-xs w-20"
          />
        </div>
      )}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Tier name"
          value={newTierName}
          onChange={(e) => setNewTierName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addTier()}
          className="h-7 text-xs"
        />
        <AddButton onClick={addTier} />
      </div>
    </Section>
  );
}

// ---------------------------------------------------------------------------
// MessagesEditor
// ---------------------------------------------------------------------------

function MessagesEditor({ tvc, setTvc }: TvcEditorProps) {
  const entries = Object.entries(tvc.messages ?? {});
  const [newId, setNewId] = useState("");

  const update = useCallback(
    (messages: TraversalConfig["messages"]) => {
      setTvc({ ...tvc, messages });
    },
    [tvc, setTvc],
  );

  const add = useCallback(() => {
    const id = newId.trim();
    if (id === "") return;
    update({
      ...tvc.messages,
      [id]: "",
    });
    setNewId("");
  }, [newId, tvc.messages, update]);

  const remove = useCallback(
    (id: string) => {
      const { [id]: _, ...rest } = tvc.messages ?? {};
      update(Object.keys(rest).length > 0 ? rest : undefined);
    },
    [tvc.messages, update],
  );

  const updateMessage = useCallback(
    (id: string, value: string) => {
      update({ ...tvc.messages, [id]: value });
    },
    [tvc.messages, update],
  );

  return (
    <Section title="Messages" count={entries.length}>
      {entries.map(([id, msg]) => (
        <div key={id} className="flex flex-col gap-1 pb-2">
          <EntryRow onRemove={() => remove(id)}>
            <span className="font-medium text-xs">{id}</span>
          </EntryRow>
          <Input
            value={msg}
            onChange={(e) => updateMessage(id, e.target.value)}
            placeholder="Message template (%points_from%, %points_to%)"
            className="h-7 text-xs ml-2"
          />
        </div>
      ))}
      <div className="flex items-center gap-2">
        <Input
          placeholder="Message ID"
          value={newId}
          onChange={(e) => setNewId(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          className="h-7 text-xs"
        />
        <AddButton onClick={add} />
      </div>
    </Section>
  );
}
