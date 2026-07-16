// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Command as CommandPrimitive } from "cmdk";
import { useEffect, useMemo, useRef, useState } from "react";
import { Command, CommandGroup, CommandItem, CommandList } from "./ui/command";
import { useTwinGraph } from "../context/NativeGraphContext";
import { cn } from "../lib/utils";

/**
 * Typeahead input for selecting node names from the current graph.
 *
 * Performs fuzzy search against graph nodes via WASM (`search_nodes_fuzzy`)
 * and renders an autocomplete dropdown using cmdk. Matched characters are
 * highlighted in the suggestion list.
 *
 * Keyboard: Arrow Up/Down to navigate, Enter to confirm, Tab/Escape to close.
 *
 * This is a controlled component — the parent owns the `value` state.
 * `onChange` fires on every keystroke; `onSelect` fires only when the user
 * picks an item from the dropdown (click or Enter).
 */

interface NodeNameInputProps {
  /** Current input value (controlled). */
  value: string;
  /** Called on every input change (typing or selection). */
  onChange: (value: string) => void;
  /** Called when the user confirms a suggestion (click or Enter). */
  onSelect?: (nodeName: string) => void;
  placeholder?: string;
  className?: string;
  /** Open the suggestion dropdown above the input instead of below. Use when
   * the input sits near the bottom of its container. */
  openUpward?: boolean;
}

export default function NodeNameInput({
  value,
  onChange,
  onSelect,
  placeholder = "Node name",
  className,
  openUpward = false,
}: NodeNameInputProps) {
  const twinGraph = useTwinGraph();
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useMemo(() => {
    if (value.length === 0) return [];
    return twinGraph.search_nodes_fuzzy(value, 8);
  }, [twinGraph, value]);

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
    <div ref={containerRef} className={cn("relative flex-1", className)}>
      <Command
        className="overflow-visible bg-transparent"
        shouldFilter={false}
        onKeyDown={(e: React.KeyboardEvent) => e.stopPropagation()}
      >
        <CommandPrimitive.Input
          ref={inputRef}
          value={value}
          onValueChange={(v) => {
            onChange(v);
            if (!isOpen && v.length > 0) {
              setIsOpen(true);
            }
          }}
          onFocus={() => {
            if (value.length > 0) setIsOpen(true);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setIsOpen(false);
              inputRef.current?.blur();
            }
            if (e.key === "Tab") {
              setIsOpen(false);
            }
          }}
          placeholder={placeholder}
          className={cn(
            "file:text-foreground placeholder:text-muted-foreground selection:bg-primary selection:text-primary-foreground dark:bg-input/30 border-input flex h-7 w-full min-w-0 rounded-md border bg-transparent px-3 py-1 text-xs shadow-xs transition-[color,box-shadow] outline-none disabled:pointer-events-none disabled:opacity-50",
            "focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
          )}
        />
        {isOpen && value.length > 0 && results.length > 0 && (
          <CommandList
            className={cn(
              "absolute border bg-card z-10 min-w-full w-max max-w-[500px] rounded-md shadow-md",
              openUpward ? "bottom-full mb-1" : "top-full mt-1",
            )}
          >
            <CommandGroup>
              {results.map((name) => (
                <CommandItem
                  key={name}
                  value={name}
                  onSelect={() => {
                    onChange(name);
                    onSelect?.(name);
                    setIsOpen(false);
                    inputRef.current?.focus();
                  }}
                  className="cursor-pointer text-xs"
                >
                  <HighlightMatch text={name} pattern={value} />
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        )}
      </Command>
    </div>
  );
}

/** Highlights the fuzzy-matched characters in `text` that correspond to `pattern`. */
function HighlightMatch({ text, pattern }: { text: string; pattern: string }) {
  const patternLower = pattern.toLowerCase();
  const textLower = text.toLowerCase();
  const parts: React.ReactNode[] = [];

  let textIdx = 0;
  let patIdx = 0;
  let lastEnd = 0;

  while (textIdx < text.length && patIdx < patternLower.length) {
    if (textLower[textIdx] === patternLower[patIdx]) {
      if (textIdx > lastEnd) {
        parts.push(
          <span key={`t-${lastEnd}`}>{text.slice(lastEnd, textIdx)}</span>,
        );
      }
      parts.push(
        <span className="font-bold text-primary" key={`m-${textIdx}`}>
          {text[textIdx]}
        </span>,
      );
      lastEnd = textIdx + 1;
      patIdx++;
    }
    textIdx++;
  }

  if (lastEnd < text.length) {
    parts.push(<span key={`t-${lastEnd}`}>{text.slice(lastEnd)}</span>);
  }

  return <span>{parts}</span>;
}
