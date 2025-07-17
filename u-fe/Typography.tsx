// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";

export function Pre({ text }: { text: string }) {
  return (
    <pre className="bg-secondary rounded-md p-2 w-full overflow-auto font-mono">
      {text}
    </pre>
  );
}

export function H1({ text, className }: { text: string; className?: string }) {
  return (
    <div
      className={clsx("text-xl font-semibold mt-2 text-foreground", className)}
    >
      {text}
    </div>
  );
}

export function H2({ text, className }: { text: string; className?: string }) {
  return (
    <div
      className={clsx("text-xl font-semibold mt-2 text-foreground", className)}
    >
      {text}
    </div>
  );
}

export function H3({ text, className }: { text: string; className?: string }) {
  return (
    <div
      className={clsx("text-lg font-semibold mt-2 text-foreground", className)}
    >
      {text}
    </div>
  );
}
