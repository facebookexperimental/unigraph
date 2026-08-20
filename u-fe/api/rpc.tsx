// Copyright (c) Meta Platforms, Inc. and affiliates.

import { createContext, use, useContext, useMemo, type ReactNode } from "react";
import type { UnigraphRequest } from "../__generated__/ts/UnigraphRequest.ts";
import type {
  UnigraphResponse,
  UnigraphResponseVariants,
} from "../__generated__/ts/UnigraphResponse.ts";

// ---------------------------------------------------------------------------
// Derived RPC type map — stays in sync with Rust via TypeGen automatically.
// No hand-maintained method list needed.
// ---------------------------------------------------------------------------

export type RpcMethod = Exclude<UnigraphResponseVariants, "Error">;

export type RpcMethodMap = {
  [M in RpcMethod]: {
    input: Extract<UnigraphRequest, Record<M, unknown>>[M];
    output: Extract<UnigraphResponse, Record<M, unknown>>[M];
  };
};

/**
 * Transport function that sends an RPC method + input and returns the output.
 * Consumers provide this when constructing an `UnigraphRpc` instance.
 */
export type RpcTransport = <M extends RpcMethod>(
  method: M,
  input: RpcMethodMap[M]["input"],
) => Promise<RpcMethodMap[M]["output"]>;

// ---------------------------------------------------------------------------
// RPC client class
// ---------------------------------------------------------------------------

export class UnigraphRpc {
  private transport: RpcTransport;

  constructor(transport: RpcTransport) {
    this.transport = transport;
  }

  call<M extends RpcMethod>(
    method: M,
    input: RpcMethodMap[M]["input"],
  ): Promise<RpcMethodMap[M]["output"]> {
    return this.transport(method, input);
  }
}

// ---------------------------------------------------------------------------
// React context + hooks
// ---------------------------------------------------------------------------

const RpcContext = createContext<UnigraphRpc | null>(null);

export function RpcProvider({
  transport,
  children,
}: {
  transport: RpcTransport;
  children: ReactNode;
}) {
  const rpc = useMemo(() => new UnigraphRpc(transport), [transport]);
  return <RpcContext value={rpc}>{children}</RpcContext>;
}

/**
 * Returns the app's `UnigraphRpc` instance from context (referentially stable
 * for the lifetime of the provider).
 *
 * RECOMMENDED USAGE: drive RPC calls through TanStack Query's `useSuspenseQuery`
 * (or `useQuery`). Do NOT call `rpc.call(...)` directly in render and hand the
 * raw promise to React's `use()` or wrap it in `useMemo`. A component that
 * suspends never commits, so its `useMemo` cache is discarded on every Suspense
 * retry — the promise, and the underlying network request, is recreated on each
 * attempt, causing a request storm. `rpc.call` itself does no caching or
 * de-duplication. TanStack Query's cache lives outside the React tree and
 * de-dupes by `queryKey`, so the request fires once:
 *
 * ```tsx
 * const rpc = useRpc();
 * const {data} = useSuspenseQuery({
 *   queryKey: ['SearchNodes', timelineId],
 *   queryFn: () =>
 *     rpc.call('SearchNodes', {timeline_id: timelineId, selection: {...}}),
 * });
 * ```
 */
export function useRpc(): UnigraphRpc {
  const rpc = useContext(RpcContext);
  if (rpc == null) {
    throw new Error("useRpc must be used within an RpcProvider");
  }
  return rpc;
}

/**
 * Suspense-compatible RPC call. Throws the promise for React to suspend on,
 * and returns the resolved output synchronously once ready.
 *
 * Must be used inside a `<Suspense>` boundary.
 *
 * ⚠️ CAVEAT: the promise is memoized with `useMemo`, which is discarded while
 * the component is suspended (a suspended component never commits). On each
 * Suspense retry the promise — and the underlying request — is recreated, so
 * the same RPC can fire many times before it resolves. Prefer TanStack Query's
 * `useSuspenseQuery` (see `useRpc`), whose cache survives outside the React tree
 * and de-dupes by `queryKey`.
 */
export function useRpcCall<M extends RpcMethod>(
  method: M,
  input: RpcMethodMap[M]["input"],
): RpcMethodMap[M]["output"] {
  const rpc = useRpc();
  const key = JSON.stringify([method, input]);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const promise = useMemo(() => rpc.call(method, input), [rpc, key]);
  return use(promise);
}

// ---------------------------------------------------------------------------
// Built-in transports
// ---------------------------------------------------------------------------

/**
 * Creates a fetch-based transport for local development.
 * Sends JSON POST requests to the given endpoint.
 */
export function createFetchTransport(endpoint = "/api/rpc"): RpcTransport {
  return async (method, input) => {
    const r = await fetch(endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ [method]: input }),
    });
    if (!r.ok) throw new Error(`RPC ${method}: HTTP ${r.status}`);
    const resp = await r.json();
    return resp[method];
  };
}
