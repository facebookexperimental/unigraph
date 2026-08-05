// Copyright (c) Meta Platforms, Inc. and affiliates.

// Filter UI for the footer's "show as a flat list" split button.
//
// Narrows the flat list to nodes matching a set of conditions: node properties,
// and the tags / dynamic edge types on a node's incoming edges. Everything is
// ANDed. Setting any condition switches the tree table's entry points to
// `Filtered`; Reset clears them and returns to `Determine`.
//
// Every input is a typeahead over choices enumerated from the graph. The one
// exception is a property with more distinct values than the backend will
// collect (`high_cardinality`), which has no options and takes freeform text.

import { Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import type { EntryPointsFilter } from "./__generated__/ts/EntryPointsFilter";
import type { PropertyCandidates } from "./__generated__/ts/PropertyCandidates";
import StringCombobox from "./components/StringCombobox";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Label } from "./components/ui/label";
import { useNativeGraphR } from "./context/NativeGraphContext";
import {
  EMPTY_ENTRY_POINTS_FILTER,
  isEntryPointsFilterEmpty,
  useEntryPointsFilter,
  useSetEntryPointsFilter,
} from "./GraphStructureHooks";
import formatNumber from "./lib/formatNumber";
import { H3 } from "./Typography";

/// Badge for the flat-list button showing how many conditions are active, so
/// the filter isn't invisible once the popover is closed.
export function FilterConditionCount() {
  const filter = useEntryPointsFilter();
  const count =
    Object.keys(filter.properties).length +
    filter.incoming_tags.length +
    filter.incoming_dynamic_type_keys.length;

  if (count === 0) {
    return null;
  }
  return <span className="text-xs tabular-nums">{count}</span>;
}

export function FlatListFilterContent() {
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

      <StringListEditor
        label="Incoming edge tags"
        values={filter.incoming_tags}
        options={candidates.tags}
        placeholder="Tag"
        onChange={(incoming_tags) => setFilter({ ...filter, incoming_tags })}
      />

      <StringListEditor
        label="Incoming dynamic edge types"
        values={filter.incoming_dynamic_type_keys}
        options={candidates.dynamic_type_keys}
        placeholder="Dynamic type key"
        onChange={(incoming_dynamic_type_keys) =>
          setFilter({ ...filter, incoming_dynamic_type_keys })
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
