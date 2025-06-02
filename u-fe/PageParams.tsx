// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, useContext, useMemo, useState } from "react";
import type { Sort } from "./tree_table/TreeTable";

export type PanelTab = "Simulation" | "None";

export type PageParams = {
  panelTab?: PanelTab;
  graphTableSort?: Sort;
};

type PageParamsContextType = [
  PageParams,
  (params: Partial<PageParams>) => void,
];

const PageParamsContext = createContext<PageParamsContextType | null>(null);

export function usePageParams(): NonNullable<PageParamsContextType> {
  const ctx = useContext(PageParamsContext);
  if (ctx == null) {
    throw new Error("usePageParams must be used within a PageParamsProvider");
  }
  return ctx;
}

export function PageParamsProvider({
  children,
  onPageParamsChange,
  initialParams,
}: {
  children: React.ReactNode;
  onPageParamsChange?: (params: PageParams) => void;
  initialParams: Partial<PageParams>;
}) {
  const [pageParams, setPageParams] = useState<PageParams>(initialParams);

  const value: PageParamsContextType = useMemo(() => {
    return [
      pageParams,
      (params: Partial<PageParams>) => {
        const newParams = { ...pageParams, ...params };
        setPageParams(newParams);
        onPageParamsChange?.(newParams);
      },
    ];
  }, [pageParams, onPageParamsChange]);

  return (
    <PageParamsContext.Provider value={value}>
      {children}
    </PageParamsContext.Provider>
  );
}
