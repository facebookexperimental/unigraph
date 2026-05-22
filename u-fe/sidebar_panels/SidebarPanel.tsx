// Copyright (c) Meta Platforms, Inc. and affiliates.
import { Separator } from "../components/ui/separator";
import { H1 } from "../Typography";

export function SidebarPanel({ children }: { children?: React.ReactNode }) {
  return (
    <div className="flex flex-col h-full w-[400px] bg-sidebar border-r overflow-y-auto p-4">
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
