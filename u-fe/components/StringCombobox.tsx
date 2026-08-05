// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Command as CommandPrimitive } from "cmdk";
import { useEffect, useMemo, useRef, useState } from "react";
import { Command, CommandGroup, CommandItem, CommandList } from "./ui/command";
import { cn } from "../lib/utils";

const MAX_SUGGESTIONS = 8;

const INPUT_CLASS_NAME =
  "file:text-foreground placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground dark:bg-input/30 border-input flex h-7 w-full min-w-0 rounded-md border bg-transparent px-3 py-1 text-xs shadow-xs transition-[color,box-shadow] outline-none disabled:pointer-events-none disabled:opacity-50 focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]";

// In normal flow rather than absolutely positioned: this renders inside a
// popover whose `overflow-y-auto` would otherwise clip a floating dropdown
// against its own edge. Growing the container instead can't be clipped, and
// the popover scrolls to the list if it needs to.
const DROPDOWN_CLASS_NAME =
  "mt-1 max-h-48 overflow-y-auto border bg-card rounded-md shadow-md";

/**
 * Typeahead over a fixed list of strings.
 *
 * Unlike `NodeNameInput`, which fuzzy-searches the whole graph through WASM,
 * this substring-filters an in-memory list the caller supplies. Freeform text
 * is still accepted, so the same component works for high-cardinality values
 * that have no suggestion list at all.
 *
 * This is a controlled component — the parent owns `value`. `onChange` fires on
 * every keystroke; `onSelect` fires only when the user commits (picks an item,
 * or presses Enter on freeform text).
 */
export default function StringCombobox({
  value,
  options,
  onChange,
  onSelect,
  placeholder,
  className,
}: {
  value: string;
  options: readonly string[];
  onChange: (value: string) => void;
  onSelect?: (value: string) => void;
  placeholder?: string;
  className?: string;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useMemo(() => {
    const query = value.trim().toLowerCase();
    const matches =
      query.length === 0
        ? options
        : options.filter((option) => option.toLowerCase().includes(query));
    return matches.slice(0, MAX_SUGGESTIONS);
  }, [options, value]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div ref={containerRef} className={cn("flex-1", className)}>
      <Command
        className="overflow-visible bg-transparent"
        shouldFilter={false}
        // Without this the global single-key shortcuts (f/r/d/e) fire while typing.
        onKeyDown={(e: React.KeyboardEvent) => e.stopPropagation()}
      >
        <CommandPrimitive.Input
          ref={inputRef}
          value={value}
          onValueChange={(v) => {
            onChange(v);
            setIsOpen(true);
          }}
          onFocus={() => setIsOpen(true)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setIsOpen(false);
              inputRef.current?.blur();
            }
            if (e.key === "Tab") {
              setIsOpen(false);
            }
            // cmdk handles Enter for the highlighted item; this only covers
            // freeform text that matches nothing in the list.
            if (e.key === "Enter" && results.length === 0) {
              onSelect?.(value);
              setIsOpen(false);
            }
          }}
          placeholder={placeholder}
          className={INPUT_CLASS_NAME}
        />
        {isOpen && results.length > 0 && (
          <CommandList className={DROPDOWN_CLASS_NAME}>
            <CommandGroup>
              {results.map((option) => (
                <CommandItem
                  key={option}
                  value={option}
                  onSelect={() => {
                    onChange(option);
                    onSelect?.(option);
                    setIsOpen(false);
                    inputRef.current?.focus();
                  }}
                  className="cursor-pointer text-xs"
                >
                  {option}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        )}
      </Command>
    </div>
  );
}
