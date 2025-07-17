// Copyright (c) Meta Platforms, Inc. and affiliates.

export function Pre({ text }: { text: string }) {
  return (
    <pre className="bg-secondary rounded-md p-2 w-full overflow-auto font-mono">
      {text}
    </pre>
  );
}
