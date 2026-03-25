// Copyright (c) Meta Platforms, Inc. and affiliates.

import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// U+10FFFF is used as a sentinel character in superroot node names
// (e.g. "􏿿__root__􏿿") to force sort-last ordering. Strip it for display
// since most fonts lack this glyph and render it inconsistently.
const SENTINEL_RE = /\u{10FFFF}/gu;

export function displayNodeName(name: string): string {
  return name.replace(SENTINEL_RE, "");
}
