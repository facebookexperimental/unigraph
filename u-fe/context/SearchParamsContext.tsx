// Copyright (c) Meta Platforms, Inc. and affiliates.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";

export interface SearchParamsContextType {
  params: Record<string, string>;
  getParam: (key: string) => string | undefined;
  setParam: (key: string, value: string) => void;
  setParams: (updates: Record<string, string>) => void;
}

const SearchParamsContext = createContext<SearchParamsContextType | null>(null);

export function SearchParamsProvider({
  children,
  initialSearchParams,
  onParamsChange,
}: {
  children: React.ReactNode;
  initialSearchParams: Record<string, string>;
  onParamsChange?: (params: Record<string, string>) => void;
}) {
  const [params, setParamsState] = useState(initialSearchParams);
  const onParamsChangeRef = useRef(onParamsChange);
  onParamsChangeRef.current = onParamsChange;

  const isInitialMount = useRef(true);
  useEffect(() => {
    if (isInitialMount.current) {
      isInitialMount.current = false;
      return;
    }
    onParamsChangeRef.current?.(params);
  }, [params]);

  const getParam = useCallback((key: string) => params[key], [params]);

  const setParam = useCallback((key: string, value: string) => {
    setParamsState((prev) => ({ ...prev, [key]: value }));
  }, []);

  const setParams = useCallback((updates: Record<string, string>) => {
    setParamsState((prev) => ({ ...prev, ...updates }));
  }, []);

  const value: SearchParamsContextType = {
    params,
    getParam,
    setParam,
    setParams,
  };

  return (
    <SearchParamsContext.Provider value={value}>
      {children}
    </SearchParamsContext.Provider>
  );
}

export function useSearchParamsContext(): SearchParamsContextType {
  const context = useContext(SearchParamsContext);
  if (context == null) {
    throw new Error(
      "useSearchParamsContext must be used within a SearchParamsProvider",
    );
  }
  return context;
}
