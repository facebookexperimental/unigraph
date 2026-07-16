// Copyright (c) Meta Platforms, Inc. and affiliates.
import { Separator } from "../components/ui/separator";
import { cn } from "../lib/utils";
import { H1 } from "../Typography";

export function SidebarPanel({
  children,
  width,
}: {
  children?: React.ReactNode;
  /** Tailwind width class (e.g. `w-[800px]`). Defaults to `w-[400px]`. */
  width?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col h-full bg-sidebar border-r overflow-y-auto p-4",
        width ?? "w-[400px]",
      )}
    >
      {children}
    </div>
  );
}

export function SidebarPanelHeader({ text }: { text: string }) {
  return (
    <>
      <H1 text={text} />
      <Separator className="my-4" />
    </>
  );
}
