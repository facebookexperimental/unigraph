// Copyright (c) Meta Platforms, Inc. and affiliates.

import { XIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./components/ui/command";
import { useNodeSearchRef, useTreeTableRef } from "./context/GlobalElementRefs";
import {
  KEYBOARD_SHORTCUTS,
  KeyboardShortcutLabel,
} from "./context/GlobalKeyboardShortcutsContext";
import { useTwinGraph } from "./context/NativeGraphContext";
import { useSelectedPath } from "./context/SelectedPathContext";
import { cn } from "./lib/utils";

export default function NodeSearch() {
  const twinGraph = useTwinGraph();
  const searchSource = useMemo(() => {
    return {
      search: (pattern: string) =>
        twinGraph
          .search_nodes_fuzzy(pattern, 8)
          .map((name) => ({ id: name, label: name })),
    };
  }, [twinGraph]);
  const selectedPath = useSelectedPath();

  return (
    <div className="h-8 grow">
      <Typeahead
        searchSource={searchSource}
        endComponent={
          <KeyboardShortcutLabel shortcut={KEYBOARD_SHORTCUTS.NODE_SEARCH} />
        }
        onSelect={(option) => {
          const nodeIDX = twinGraph.getNodeIDXByNameLog(option.id);
          if (nodeIDX != null) {
            selectedPath.setSelectedPath([nodeIDX], true);
          }
        }}
      />
    </div>
  );
}

export interface TypeaheadEntry {
  id: string;
  label: string;
}

export interface TypeaheadSearchSource {
  search(pattern: string): TypeaheadEntry[];
}

interface TypeaheadProps {
  /** Search source that provides the search functionality */
  searchSource: TypeaheadSearchSource;
  /** Placeholder text for the input */
  placeholder?: string;
  /** Current selected value */
  value?: string;
  /** Callback when value changes */
  onValueChange?: (value: string | undefined) => void;
  /** Callback when an option is selected */
  onSelect?: (option: TypeaheadEntry) => void;
  /** Whether the component is disabled */
  disabled?: boolean;
  /** Custom className for the container */
  className?: string;
  /** Text to show when no results are found */
  emptyText?: string;
  /** Minimum characters required before searching */
  minSearchLength?: number;
  /** Debounce delay in milliseconds */
  debounceMs?: number;
  /** Whether to allow clearing the selection */
  clearable?: boolean;
  /** Component to render at the end of the search box (e.g., keyboard shortcut label) */
  endComponent?: React.ReactNode;
}

// Helper function to highlight matching characters in fuzzy search
function highlightMatches(
  text: string,
  searchPattern: string,
): React.ReactNode {
  if (!searchPattern) return text;

  const pattern = searchPattern.toLowerCase();
  const textLower = text.toLowerCase();
  const result: React.ReactNode[] = [];

  let textIndex = 0;
  let patternIndex = 0;
  let lastMatchEnd = 0;

  while (textIndex < text.length && patternIndex < pattern.length) {
    if (textLower[textIndex] === pattern[patternIndex]) {
      // Add non-matching text before this match
      if (textIndex > lastMatchEnd) {
        result.push(
          <span key={`text-${lastMatchEnd}`}>
            {text.slice(lastMatchEnd, textIndex)}
          </span>,
        );
      }

      // Add the matching character in bold
      result.push(
        <span
          className="font-bold text-primary"
          key={`match-${textIndex}-${patternIndex}`}
        >
          {text[textIndex]}
        </span>,
      );

      lastMatchEnd = textIndex + 1;
      patternIndex++;
    }
    textIndex++;
  }

  // Add any remaining text
  if (lastMatchEnd < text.length) {
    result.push(
      <span key={`text-${lastMatchEnd}`}>{text.slice(lastMatchEnd)}</span>,
    );
  }

  return <p>{result}</p>;
}

function Typeahead({
  searchSource,
  placeholder = "Search...",
  value,
  onValueChange,
  onSelect,
  disabled = false,
  className,
  emptyText = "No results found",
  minSearchLength = 1,
  debounceMs = 500,
  clearable = true,
  endComponent,
}: TypeaheadProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [searchValue, setSearchValue] = useState("");

  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useNodeSearchRef();
  const debounceTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const treeTableRef = useTreeTableRef();

  // Debounced search results
  const searchResults = useMemo(() => {
    if (searchValue.length < minSearchLength) {
      return [];
    }
    return searchSource.search(searchValue);
  }, [searchSource, searchValue, minSearchLength]);

  // Handle debounced search
  const debouncedSearch = useCallback(
    (query: string) => {
      if (debounceTimeoutRef.current) {
        clearTimeout(debounceTimeoutRef.current);
      }

      debounceTimeoutRef.current = setTimeout(() => {
        setSearchValue(query);
      }, debounceMs);
    },
    [debounceMs],
  );

  // Update selected option when value prop changes
  useEffect(() => {
    if (value !== undefined) {
      const option = searchSource.search("").find((opt) => opt.id === value);
      if (option) {
        setSearchValue(option.label);
      }
    } else {
      setSearchValue("");
    }
  }, [value, searchSource]);

  // Handle input change
  const handleInputChange = (inputValue: string) => {
    setSearchValue(inputValue);
    debouncedSearch(inputValue);
    if (!isOpen && inputValue.length >= minSearchLength) {
      setIsOpen(true);
    }
  };

  // Handle option selection
  const handleSelect = (option: TypeaheadEntry) => {
    if (debounceTimeoutRef.current) {
      clearTimeout(debounceTimeoutRef.current);
    }

    setIsOpen(false);
    onValueChange?.(option.id);
    onSelect?.(option);
    // Focus on tree table so after selecting the
    // navigation shortcuts can start working again (up, down, right)
    // after the node was selected
    treeTableRef.current?.focus();
  };

  // Handle clear
  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSearchValue("");
    onValueChange?.(undefined);
    setIsOpen(false);
    inputRef.current?.focus();
  };

  // Handle click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
        // Reset search value to selected option label if user clicked outside
        if (!searchValue) {
          setSearchValue("");
        }
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [searchValue]);

  // Handle keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      setIsOpen(false);
      inputRef.current?.blur();
    } else if (e.key === "ArrowDown" && !isOpen) {
      setIsOpen(true);
    }
  };

  // Cleanup debounce timeout on unmount
  useEffect(() => {
    return () => {
      if (debounceTimeoutRef.current) {
        clearTimeout(debounceTimeoutRef.current);
      }
    };
  }, []);

  return (
    <div ref={containerRef} className={cn("relative w-full", className)}>
      <div className="relative">
        <Command
          className="border border-input bg-background"
          shouldFilter={false} // We handle filtering ourselves
          onKeyDown={(e: React.KeyboardEvent) => e.stopPropagation()} // prevent global keyboard shortcuts
        >
          <div className="flex items-center px-3 w-full" cmdk-input-wrapper="">
            <CommandInput
              ref={inputRef}
              placeholder={placeholder}
              value={searchValue}
              onValueChange={handleInputChange}
              onKeyDown={handleKeyDown}
              onFocus={() => {
                if (searchValue.length >= minSearchLength) {
                  setIsOpen(true);
                }
              }}
              disabled={disabled}
              className="flex h-10 w-full rounded-md bg-transparent py-3 text-sm outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50"
            />

            <div className="flex items-center gap-1">
              {endComponent}
              {clearable && (
                <button
                  type="button"
                  onClick={handleClear}
                  className="h-4 w-4 opacity-50 hover:opacity-100"
                  disabled={disabled}
                >
                  {searchValue.length !== 0 && (
                    <XIcon className="h-4 w-4 cursor-pointer" />
                  )}
                </button>
              )}
            </div>
          </div>

          {searchValue.length !== 0 && isOpen && (
            <CommandList className="absolute bottom-full mb-1 border bg-card z-10 w-full rounded-md shadow-md">
              {searchValue.length !== 0 && searchResults.length === 0 ? (
                <CommandEmpty>{emptyText}</CommandEmpty>
              ) : (
                <CommandGroup>
                  {searchResults.map((option) => (
                    <CommandItem
                      key={option.id}
                      value={option.id}
                      onSelect={() => handleSelect(option)}
                      className="cursor-pointer"
                    >
                      {highlightMatches(option.label, searchValue)}
                    </CommandItem>
                  ))}
                </CommandGroup>
              )}
            </CommandList>
          )}
        </Command>
      </div>
    </div>
  );
}
