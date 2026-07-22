// Copyright (c) Meta Platforms, Inc. and affiliates.
import { ResizeHandle, useResizableWidth } from "../components/ResizeHandle";
import { Separator } from "../components/ui/separator";
import { H1 } from "../Typography";

export function SidebarPanel({
  children,
  storageKey = "sidebar",
  defaultWidth = 400,
}: {
  children?: React.ReactNode;
  /** localStorage key so each panel remembers its own width. */
  storageKey?: string;
  /** Initial width in px, used until the user drags the resize handle. */
  defaultWidth?: number;
}) {
  const { width, handleProps } = useResizableWidth(storageKey, defaultWidth);

  return (
    <div
      className="relative flex flex-col h-full bg-sidebar border-r"
      style={{ width }}
    >
      <div className="flex flex-col h-full overflow-y-auto p-4">{children}</div>
      <ResizeHandle {...handleProps} />
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
