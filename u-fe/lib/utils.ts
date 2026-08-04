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

/// Save a Blob to disk by clicking a synthetic `<a download>`. Entirely
/// client-side, so it works for hundreds of MB without a server round-trip.
export function triggerDownload(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
