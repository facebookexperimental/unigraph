// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Separator } from "../components/ui/separator";

export function SidebarPanel({
  children,
}: {
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col h-full w-[400px] bg-sidebar border-r overflow-y-auto p-4">
      {children}
    </div>
  );
}

export function SidebarPanelHeader({
  children,
}: {
  children?: React.ReactNode;
}) {
  return (
    <>
      <h2 className="text-3xl font-bold">{children}</h2>
      <Separator className="my-4" />
    </>
  );
}
