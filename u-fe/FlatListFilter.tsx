// Copyright (c) Meta Platforms, Inc. and affiliates.

// The footer's flat-list filter: a split button plus the popover that edits it.
//
// Narrows the flat list to nodes matching a set of conditions: node name, node
// properties, and the tags / dynamic edge types on a node's incoming and
// outgoing edges. Everything is ANDed.
//
// The two halves split "is the filter applied?" from "what is the filter?". The
// toggle half flips the tree table between `AllReachable` and `Filtered` and is
// inert until conditions exist, since both positions would otherwise render the
// same rows. The popover half is always live — it's the only way to get those
// conditions in the first place.
//
// Nothing in the popover commits on its own: the whole thing is a draft, and
// only Apply pushes it into graph settings.
//
// The node name field forced that. Committing text per keystroke would re-run
// the filter each time, and on a large graph that's a full scan of every
// reachable node. Once one input has to be deferred, deferring the rest costs
// nothing and means Apply is the one place anything takes effect.

import { Filter, Trash2, X } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import type { NodeSelection } from "./__generated__/ts/NodeSelection";
import type { FilterCandidates } from "./__generated__/ts/FilterCandidates";
import type { NameMatch } from "./__generated__/ts/NameMatch";
import type { NameMatchMode } from "./__generated__/ts/NameMatchMode";
import type { PropertyCandidates } from "./__generated__/ts/PropertyCandidates";
import StringCombobox from "./components/StringCombobox";
import USplitToggleButton from "./components/USplitToggleButton";
import UTooltip from "./components/UTooltip";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { useNativeGraphR } from "./context/NativeGraphContext";
import { cn } from "./lib/utils";
import { validateNameMatch } from "./native/NativeGraph";
import {
  countEntryPointsFilterConditions,
  EMPTY_ENTRY_POINTS_FILTER,
  hasNameCondition,
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

  // Owned here, not in the popover: the popover's content unmounts when it
  // closes, and a mis-click shouldn't throw away a half-typed pattern.
  const draft = useFilterDraft();

  return (
    <USplitToggleButton
      tooltip={filterTooltip(conditionCount, filtering)}
      selected={filtering}
      onSelectedChange={toggleFiltering}
      // Disabled exactly when pressing it would be a no-op. Turning filtering
      // off never is — a graph can ship `Filtered` with no conditions, and that
      // must not be a state you're locked into.
      toggleDisabled={conditionCount === 0 && !filtering}
      popoverContent={<FlatListFilterContent state={draft} />}
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

function FlatListFilterContent({ state }: { state: FilterDraft }) {
  const { draft, setDraft, nameError, dirty, apply, reset, canReset } = state;
  const applied = useEntryPointsFilter();
  const nativeGraph = useNativeGraphR();

  // Lazy on the WASM side, so it only ever runs once this popover is opened.
  const candidates = useMemo(
    () => nativeGraph.filterCandidates(),
    [nativeGraph],
  );

  // Counts what's *applied*, not what's drafted — it describes the table you're
  // looking at. Previewing the draft would mean re-running the filter on every
  // edit, which is exactly what Apply exists to avoid.
  const matchCount = isEntryPointsFilterEmpty(applied)
    ? null
    : nativeGraph.filteredEntrypoints(applied).vec.length;

  // Every graph has node names, so unlike the other editors this one is never
  // empty — only the metadata-driven sections can have nothing to offer.
  const hasMetadataToFilterOn =
    candidates.properties.length > 0 ||
    candidates.tags.length > 0 ||
    candidates.dynamic_type_keys.length > 0;

  return (
    <div className="flex flex-col gap-3">
      <H3 text="Filter flat list" />

      <NameFilterEditor
        name={draft.name ?? EMPTY_NAME_MATCH}
        error={nameError}
        onChange={(name) => setDraft({ ...draft, name })}
        onSubmit={apply}
      />

      {!hasMetadataToFilterOn && (
        <p className="text-xs text-muted-foreground">
          This graph has no node properties, edge tags or dynamic edge types, so
          name is the only thing to filter on.
        </p>
      )}

      <PropertyFilterEditor
        filter={draft}
        candidates={candidates.properties}
        onChange={setDraft}
      />

      <EdgeFilterEditor
        direction="Incoming"
        tags={draft.incoming_tags}
        dynamicTypeKeys={draft.incoming_dynamic_type_keys}
        candidates={candidates}
        onChangeTags={(incoming_tags) => setDraft({ ...draft, incoming_tags })}
        onChangeDynamicTypeKeys={(incoming_dynamic_type_keys) =>
          setDraft({ ...draft, incoming_dynamic_type_keys })
        }
      />

      <EdgeFilterEditor
        direction="Outgoing"
        tags={draft.outgoing_tags}
        dynamicTypeKeys={draft.outgoing_dynamic_type_keys}
        candidates={candidates}
        onChangeTags={(outgoing_tags) => setDraft({ ...draft, outgoing_tags })}
        onChangeDynamicTypeKeys={(outgoing_dynamic_type_keys) =>
          setDraft({ ...draft, outgoing_dynamic_type_keys })
        }
      />

      <div className="flex items-center justify-between gap-2 border-t pt-2">
        <span className="text-xs text-muted-foreground">
          {appliedSummary(matchCount, dirty)}
        </span>
        <div className="flex items-center gap-2 shrink-0">
          <Button
            size="sm"
            variant="outline"
            className="cursor-pointer text-xs h-7"
            disabled={!canReset}
            onClick={reset}
          >
            Reset
          </Button>
          <UTooltip tooltip={applyTooltip(nameError, dirty)}>
            <Button
              size="sm"
              // `aria-disabled`, not `disabled`: a disabled button drops
              // pointer events, and the tooltip is what explains the greying.
              aria-disabled={!dirty}
              className={cn(
                "text-xs h-7",
                dirty ? "cursor-pointer" : "opacity-50 cursor-default",
              )}
              onClick={() => {
                if (dirty) {
                  apply();
                }
              }}
            >
              Apply
            </Button>
          </UTooltip>
        </div>
      </div>
    </div>
  );
}

/// Says what the table currently shows, and flags edits that aren't in it yet —
/// without that the count looks stale once you start editing.
function appliedSummary(matchCount: number | null, dirty: boolean): string {
  const summary =
    matchCount == null
      ? "No filter applied — showing every node"
      : `${formatNumber(matchCount)} matching nodes`;
  return dirty ? `${summary} · unapplied changes` : summary;
}

function applyTooltip(nameError: string | null, dirty: boolean): string {
  if (nameError != null) {
    return "Fix the node name pattern before applying";
  }
  return dirty
    ? "Apply these conditions to the flat list"
    : "Nothing to apply — the conditions below are already in effect";
}

const RUST_REGEX_DOCS = "https://docs.rs/regex/latest/regex/";

const EMPTY_NAME_MATCH: NameMatch = { pattern: "", mode: "Substring" };

/// The popover's pending conditions, and the two ways out of them.
type FilterDraft = {
  draft: NodeSelection;
  setDraft: (draft: NodeSelection) => void;
  /// What the regex parser said about the name pattern, or `null` if it's fine.
  nameError: string | null;
  /// Applying would change something, and nothing in the draft is invalid.
  dirty: boolean;
  apply: () => void;
  /// Clears the draft *and* the applied filter. Unlike Apply this takes effect
  /// immediately — "reset, then press Apply" is a strange thing to ask of a
  /// button whose whole job is getting back to a clean slate.
  reset: () => void;
  canReset: boolean;
};

function useFilterDraft(): FilterDraft {
  const committed = useEntryPointsFilter();
  const setFilter = useSetEntryPointsFilter();
  const [draft, setDraft] = useState(committed);

  // Adopt the committed filter when something *else* changed it. Comparing
  // against the last value we saw, rather than against the draft, is what stops
  // this from undoing edits in progress.
  const committedKey = filterKey(committed);
  const lastSeen = useRef(committedKey);
  if (lastSeen.current !== committedKey) {
    lastSeen.current = committedKey;
    setDraft(committed);
  }

  const nameError =
    draft.name != null && hasNameCondition(draft)
      ? validateNameMatch(draft.name)
      : null;

  return {
    draft,
    setDraft,
    nameError,
    dirty: nameError == null && filterKey(draft) !== committedKey,
    apply: () => {
      if (nameError != null) {
        return;
      }
      setFilter({
        ...draft,
        // Don't persist a pattern of pure whitespace: it reads as a condition
        // in the settings blob but matches everything.
        name: hasNameCondition(draft) ? draft.name : undefined,
      });
    },
    reset: () => {
      setDraft(EMPTY_ENTRY_POINTS_FILTER);
      setFilter(EMPTY_ENTRY_POINTS_FILTER);
    },
    canReset:
      !isEntryPointsFilterEmpty(draft) || !isEntryPointsFilterEmpty(committed),
  };
}

/// Stable identity for dirty-checking.
///
/// Can't just `JSON.stringify` the filter: the string lists are sets in meaning
/// but arrays on the wire, property key order is incidental, and a blank name
/// pattern is no condition at all. Without normalizing those, Apply lights up
/// for edits that would change nothing.
function filterKey(filter: NodeSelection): string {
  return JSON.stringify([
    hasNameCondition(filter) ? [filter.name?.pattern, filter.name?.mode] : null,
    Object.entries(filter.properties)
      .map(([name, match]) => [name, match.value ?? null])
      .sort(),
    [...filter.incoming_tags].sort(),
    [...filter.incoming_dynamic_type_keys].sort(),
    [...filter.outgoing_tags].sort(),
    [...filter.outgoing_dynamic_type_keys].sort(),
  ]);
}

function NameFilterEditor({
  name,
  error,
  onChange,
  onSubmit,
}: {
  name: NameMatch;
  error: string | null;
  onChange: (name: NameMatch) => void;
  onSubmit: () => void;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label className="text-xs text-muted-foreground">Node name</Label>
      <div className="flex items-center gap-2">
        <Input
          value={name.pattern}
          onChange={(e) => onChange({ ...name, pattern: e.target.value })}
          // Without this the global single-key shortcuts (f/r/d/e) fire while typing.
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") {
              onSubmit();
            }
          }}
          placeholder={name.mode === "Regex" ? "^foo/.*bar$" : "Name contains…"}
          aria-invalid={error != null}
          className="h-7 text-xs"
        />
        <NameModeToggle
          mode={name.mode}
          onChange={(mode) => onChange({ ...name, mode })}
        />
      </div>
      {error != null && (
        <p className="text-xs text-destructive break-words">{error}</p>
      )}
    </div>
  );
}

/// Equal-width halves so the pair reads as one control rather than two buttons
/// that happen to touch.
function NameModeToggle({
  mode,
  onChange,
}: {
  mode: NameMatchMode;
  onChange: (mode: NameMatchMode) => void;
}) {
  return (
    <div className="inline-flex shrink-0">
      <UTooltip tooltip="Plain text, matched anywhere in the name, ignoring case">
        <Button
          size="sm"
          variant={mode === "Substring" ? "default" : "secondary"}
          className="cursor-pointer rounded-r-none h-7 w-12 px-0 text-xs relative focus-visible:z-10"
          onClick={() => onChange("Substring")}
        >
          Text
        </Button>
      </UTooltip>
      <UTooltip tooltip={<RegexModeTooltip />}>
        <Button
          size="sm"
          variant={mode === "Regex" ? "default" : "secondary"}
          className="cursor-pointer rounded-l-none h-7 w-12 px-0 text-xs relative focus-visible:z-10"
          onClick={() => onChange("Regex")}
        >
          .*
        </Button>
      </UTooltip>
    </div>
  );
}

function RegexModeTooltip() {
  return (
    <span className="flex flex-col gap-1">
      <span>
        Rust regex syntax — unanchored and case-sensitive. Use <code>(?i)</code>{" "}
        to fold case, <code>^</code>/<code>$</code> to anchor.
      </span>
      <a
        href={RUST_REGEX_DOCS}
        target="_blank"
        rel="noreferrer"
        className="underline underline-offset-2"
      >
        Syntax reference ↗
      </a>
    </span>
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
  filter: NodeSelection;
  candidates: PropertyCandidates[];
  onChange: (filter: NodeSelection) => void;
}) {
  const [draftName, setDraftName] = useState("");
  // The row that was just added, so focus can move to its value field. Picking
  // a name is only half the condition, and leaving the caret in "Add a
  // property" means the next thing typed starts a second row instead.
  const [justAdded, setJustAdded] = useState<string | null>(null);

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
    setJustAdded(trimmed);
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
        // `items-start`, not `items-center`: the combobox grows downwards when
        // its dropdown opens, and centring would drag the badge and the delete
        // button halfway down the list. Matching the input's `h-7` keeps them
        // lined up with it either way.
        <div className="flex items-start gap-2" key={name}>
          <Badge
            variant="default"
            className="text-xs shrink-0 max-w-32 h-7 whitespace-nowrap"
          >
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
            // Fires on mount, which is exactly when the row appears — so this
            // hands the caret over the moment a property is picked, and never
            // steals it back afterwards.
            autoFocus={name === justAdded}
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
          // Focus goes to the new row's value field instead, so this one must
          // not grab it back — otherwise both dropdowns end up open at once.
          refocusOnSelect={false}
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
            // Primary, not secondary: a chosen condition should stand out from
            // the empty inputs around it, which are all muted greys.
            <Badge key={value} variant="default" className="text-xs gap-1">
              {value}
              <button
                type="button"
                aria-label={`Remove ${value}`}
                className="cursor-pointer opacity-70 hover:opacity-100"
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
