// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";

export function Pre({ text, className }: { text: string; className?: string }) {
  return (
    <pre
      className={clsx(
        "bg-secondary rounded-md p-2 w-full overflow-auto font-mono",
        className,
      )}
    >
      {text}
    </pre>
  );
}

export function H1({ text, className }: { text: string; className?: string }) {
  return (
    <div className={clsx("text-xl font-semibold text-foreground", className)}>
      {text}
    </div>
  );
}

export function H2({ text, className }: { text: string; className?: string }) {
  return (
    <div className={clsx("text-xl font-semibold text-foreground", className)}>
      {text}
    </div>
  );
}

export function H3({ text, className }: { text: string; className?: string }) {
  return (
    <div className={clsx("text-lg font-semibold text-foreground", className)}>
      {text}
    </div>
  );
}

export function P({ text, className }: { text: string; className?: string }) {
  return <div className={clsx("text-foreground", className)}>{text}</div>;
}

export function Link({
  text,
  href,
  target = "_blank",
  className,
}: {
  text: string;
  href: string;
  target?: React.HTMLAttributeAnchorTarget;
  className?: string;
}) {
  return (
    <a
      href={href}
      target={target}
      className={clsx("text-primary hover:underline break-all", className)}
    >
      {text}
    </a>
  );
}
