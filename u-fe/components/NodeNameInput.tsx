// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Command as CommandPrimitive } from "cmdk";
import { useEffect, useMemo, useRef, useState } from "react";
import { Command, CommandGroup, CommandItem, CommandList } from "./ui/command";
import { useTwinGraph } from "../context/NativeGraphContext";
import { cn } from "../lib/utils";

interface NodeNameInputProps {
  value: string;
  onChange: (value: string) => void;
  onSelect?: (nodeName: string) => void;
  placeholder?: string;
  className?: string;
}

export default function NodeNameInput({
  value,
  onChange,
  onSelect,
  placeholder = "Node name",
  className,
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
          <CommandList className="absolute top-full mt-1 border bg-card z-10 min-w-full w-max max-w-[500px] rounded-md shadow-md">
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
