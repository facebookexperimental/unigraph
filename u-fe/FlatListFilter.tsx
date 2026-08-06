// Copyright (c) Meta Platforms, Inc. and affiliates.

// The footer's flat-list filter: a split button plus the popover that edits it.
//
// Narrows the flat list to nodes matching a set of conditions: node properties,
// and the tags / dynamic edge types on a node's incoming and outgoing edges.
// Everything is ANDed.
//
// The two halves split "is the filter applied?" from "what is the filter?". The
// toggle half flips the tree table between `AllReachable` and `Filtered` and is
// inert until conditions exist, since both positions would otherwise render the
// same rows. The popover half is always live — it's the only way to get those
// conditions in the first place.
//
// Every input is a typeahead over choices enumerated from the graph. The one
// exception is a property with more distinct values than the backend will
// collect (`high_cardinality`), which has no options and takes freeform text.

import { Filter, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { EntryPointsFilter } from "./__generated__/ts/EntryPointsFilter";
import type { FilterCandidates } from "./__generated__/ts/FilterCandidates";
import type { PropertyCandidates } from "./__generated__/ts/PropertyCandidates";
import StringCombobox from "./components/StringCombobox";
import USplitToggleButton from "./components/USplitToggleButton";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Label } from "./components/ui/label";
import { useNativeGraphR } from "./context/NativeGraphContext";
import {
  countEntryPointsFilterConditions,
  EMPTY_ENTRY_POINTS_FILTER,
  isEntryPointsFilterEmpty,
  useEntryPointsFilter,
  useSetEntryPointsFilter,
  useToggleFilteredFlatList,
} from "./GraphStructureHooks";
import formatNumber from "./lib/formatNumber";
import { H3 } from "./Typography";

export function FlatListFilterButton() {
  const conditionCount = countEntryPointsFilterConditions(
    useEntryPointsFilter(),
  );
  const [filtering, toggleFiltering] = useToggleFilteredFlatList();

  return (
    <USplitToggleButton
      tooltip={filterTooltip(conditionCount, filtering)}
      selected={filtering}
      onSelectedChange={toggleFiltering}
      // Disabled exactly when pressing it would be a no-op. Turning filtering
      // off never is — a graph can ship `Filtered` with no conditions, and that
      // must not be a state you're locked into.
      toggleDisabled={conditionCount === 0 && !filtering}
      popoverContent={<FlatListFilterContent />}
      popoverClassName="w-[32rem] max-h-[85vh]"
    >
      <Filter />
      {conditionCount > 0 && (
        <span className="text-xs tabular-nums">{conditionCount}</span>
      )}
    </USplitToggleButton>
  );
}

/// The disabled state has to say what to do about it — an unexplained dead
/// button is the whole reason this spells each case out.
function filterTooltip(conditionCount: number, filtering: boolean) {
  const conditions = `${conditionCount} ${conditionCount === 1 ? "condition" : "conditions"}`;

  if (filtering) {
    const what =
      conditionCount === 0
        ? "Filtering with no conditions"
        : `Filtered by ${conditions}`;
    return `${what} — click to show the whole flat list`;
  }
  return conditionCount === 0
    ? "Filter the flat list — open the dropdown to add conditions"
    : `Narrow the flat list to the nodes matching ${conditions}`;
}

function FlatListFilterContent() {
  const filter = useEntryPointsFilter();
  const setFilter = useSetEntryPointsFilter();
  const nativeGraph = useNativeGraphR();

  // Lazy on the WASM side, so it only ever runs once this popover is opened.
  const candidates = useMemo(
    () => nativeGraph.filterCandidates(),
    [nativeGraph],
  );

  const isEmpty = isEntryPointsFilterEmpty(filter);
  const matchCount = isEmpty
    ? null
    : nativeGraph.filteredEntrypoints(filter).vec.length;

  const hasAnythingToFilterOn =
    candidates.properties.length > 0 ||
    candidates.tags.length > 0 ||
    candidates.dynamic_type_keys.length > 0;

  if (!hasAnythingToFilterOn) {
    return (
      <div className="flex flex-col gap-2">
        <H3 text="Filter flat list" />
        <p className="text-xs text-muted-foreground">
          This graph has no node properties, edge tags or dynamic edge types to
          filter on.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <H3 text="Filter flat list" />

      <PropertyFilterEditor
        filter={filter}
        candidates={candidates.properties}
        onChange={setFilter}
      />

      <EdgeFilterEditor
        direction="Incoming"
        tags={filter.incoming_tags}
        dynamicTypeKeys={filter.incoming_dynamic_type_keys}
        candidates={candidates}
        onChangeTags={(incoming_tags) =>
          setFilter({ ...filter, incoming_tags })
        }
        onChangeDynamicTypeKeys={(incoming_dynamic_type_keys) =>
          setFilter({ ...filter, incoming_dynamic_type_keys })
        }
      />

      <EdgeFilterEditor
        direction="Outgoing"
        tags={filter.outgoing_tags}
        dynamicTypeKeys={filter.outgoing_dynamic_type_keys}
        candidates={candidates}
        onChangeTags={(outgoing_tags) =>
          setFilter({ ...filter, outgoing_tags })
        }
        onChangeDynamicTypeKeys={(outgoing_dynamic_type_keys) =>
          setFilter({ ...filter, outgoing_dynamic_type_keys })
        }
      />

      <div className="flex items-center justify-between border-t pt-2">
        <span className="text-xs text-muted-foreground">
          {matchCount == null
            ? "No conditions — showing every node"
            : `${formatNumber(matchCount)} matching nodes`}
        </span>
        <Button
          size="sm"
          variant="ghost"
          className="cursor-pointer text-xs h-7"
          disabled={isEmpty}
          onClick={() => setFilter(EMPTY_ENTRY_POINTS_FILTER)}
        >
          Reset
        </Button>
      </div>
    </div>
  );
}

/// Property conditions. Each committed row keeps its name fixed (remove and
/// re-add to change it) and lets the value be edited or cleared, where an
/// empty value means "has this property, whatever its value".
function PropertyFilterEditor({
  filter,
  candidates,
  onChange,
}: {
  filter: EntryPointsFilter;
  candidates: PropertyCandidates[];
  onChange: (filter: EntryPointsFilter) => void;
}) {
  const [draftName, setDraftName] = useState("");

  if (candidates.length === 0) {
    return null;
  }

  const entries = Object.entries(filter.properties);
  const usedNames = new Set(entries.map(([name]) => name));
  const availableNames = candidates
    .map((candidate) => candidate.name)
    .filter((name) => !usedNames.has(name));

  const addProperty = (name: string) => {
    const trimmed = name.trim();
    setDraftName("");
    if (trimmed.length === 0 || usedNames.has(trimmed)) {
      return;
    }
    onChange({
      ...filter,
      properties: { ...filter.properties, [trimmed]: {} },
    });
  };

  const setValue = (name: string, value: string) => {
    onChange({
      ...filter,
      properties: {
        ...filter.properties,
        [name]: value.length === 0 ? {} : { value },
      },
    });
  };

  const removeProperty = (name: string) => {
    const { [name]: _removed, ...rest } = filter.properties;
    onChange({ ...filter, properties: rest });
  };

  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-xs text-muted-foreground">Properties</Label>
      {entries.map(([name, valueMatch]) => (
        <div className="flex items-center gap-2" key={name}>
          <Badge variant="outline" className="text-xs shrink-0 max-w-32">
            <span className="truncate">{name}</span>
          </Badge>
          <StringCombobox
            value={valueMatch.value ?? ""}
            options={
              candidates.find((candidate) => candidate.name === name)?.values ??
              []
            }
            onChange={(value) => setValue(name, value)}
            placeholder="Any value"
          />
          <Button
            size="icon"
            variant="ghost"
            aria-label={`Remove ${name} condition`}
            className="cursor-pointer size-7 shrink-0"
            onClick={() => removeProperty(name)}
          >
            <Trash2 className="size-3" />
          </Button>
        </div>
      ))}
      {availableNames.length > 0 && (
        <StringCombobox
          value={draftName}
          options={availableNames}
          onChange={setDraftName}
          onSelect={addProperty}
          placeholder="Add a property"
        />
      )}
    </div>
  );
}

/// Conditions on one direction's edges. The candidate lists are graph-wide
/// rather than per-direction, so a choice here can legitimately match nothing —
/// a tag that only ever appears on root's outgoing edges, say.
function EdgeFilterEditor({
  direction,
  tags,
  dynamicTypeKeys,
  candidates,
  onChangeTags,
  onChangeDynamicTypeKeys,
}: {
  direction: "Incoming" | "Outgoing";
  tags: string[];
  dynamicTypeKeys: string[];
  candidates: FilterCandidates;
  onChangeTags: (tags: string[]) => void;
  onChangeDynamicTypeKeys: (dynamicTypeKeys: string[]) => void;
}) {
  return (
    <>
      <StringListEditor
        label={`${direction} edge tags`}
        values={tags}
        options={candidates.tags}
        placeholder="Tag"
        onChange={onChangeTags}
      />

      <StringListEditor
        label={`${direction} dynamic edge types`}
        values={dynamicTypeKeys}
        options={candidates.dynamic_type_keys}
        placeholder="Dynamic type key"
        onChange={onChangeDynamicTypeKeys}
      />
    </>
  );
}

/// A set of chosen strings rendered as removable badges, plus a typeahead to
/// add more. Renders nothing when the graph has no candidates of this kind.
function StringListEditor({
  label,
  values,
  options,
  placeholder,
  onChange,
}: {
  label: string;
  values: string[];
  options: string[];
  placeholder: string;
  onChange: (values: string[]) => void;
}) {
  const [draft, setDraft] = useState("");

  if (options.length === 0) {
    return null;
  }

  const add = (value: string) => {
    const trimmed = value.trim();
    setDraft("");
    if (trimmed.length === 0 || values.includes(trimmed)) {
      return;
    }
    onChange([...values, trimmed]);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-xs text-muted-foreground">{label}</Label>
      {values.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {values.map((value) => (
            <Badge key={value} variant="secondary" className="text-xs gap-1">
              {value}
              <button
                type="button"
                aria-label={`Remove ${value}`}
                className="cursor-pointer"
                onClick={() => onChange(values.filter((v) => v !== value))}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))}
        </div>
      )}
      <StringCombobox
        value={draft}
        options={options.filter((option) => !values.includes(option))}
        onChange={setDraft}
        onSelect={add}
        placeholder={placeholder}
      />
    </div>
  );
}
