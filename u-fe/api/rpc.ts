// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { GetConfigsInput } from "../__generated__/ts/GetConfigsInput.ts";
import type { GetConfigsOutput } from "../__generated__/ts/GetConfigsOutput.ts";
import type { GraphQueryInput } from "../__generated__/ts/GraphQueryInput.ts";
import type { GraphQueryOutput } from "../__generated__/ts/GraphQueryOutput.ts";
import type { ListTimelinesInput } from "../__generated__/ts/ListTimelinesInput.ts";
import type { ListTimelinesOutput } from "../__generated__/ts/ListTimelinesOutput.ts";
import type { PutConfigsInput } from "../__generated__/ts/PutConfigsInput.ts";
import type { PutConfigsOutput } from "../__generated__/ts/PutConfigsOutput.ts";
import type { SelectFramesInput } from "../__generated__/ts/SelectFramesInput.ts";
import type { SelectFramesOutput } from "../__generated__/ts/SelectFramesOutput.ts";

type RpcMap = {
  PutConfigs: { input: PutConfigsInput; output: PutConfigsOutput };
  GetConfigs: { input: GetConfigsInput; output: GetConfigsOutput };
  GraphQuery: { input: GraphQueryInput; output: GraphQueryOutput };
  ListTimelines: { input: ListTimelinesInput; output: ListTimelinesOutput };
  SelectFrames: { input: SelectFramesInput; output: SelectFramesOutput };
};

export type RpcMethod = keyof RpcMap;
export type RpcInput<M extends RpcMethod> = RpcMap[M]["input"];
export type RpcOutput<M extends RpcMethod> = RpcMap[M]["output"];

/**
 * Transport function that consumers must provide to `initRpc`.
 * Takes a method name and typed input, returns the typed output.
 */
export type RpcTransport = <M extends RpcMethod>(
  method: M,
  input: RpcInput<M>,
) => Promise<RpcOutput<M>>;

let _transport: RpcTransport | null = null;

/**
 * Initialize the RPC layer with a transport implementation.
 * Must be called before any `call_rpc` calls.
 *
 * Example (fetch-based):
 *   initRpc(createFetchTransport("/api/rpc"))
 *
 * Example (custom):
 *   initRpc(async (method, input) => myBackend.call(method, input))
 */
export function initRpc(transport: RpcTransport): void {
  _transport = transport;
}

/**
 * Call an RPC method using the configured transport.
 * Throws if `initRpc` has not been called.
 */
export async function callUnigraphRPC<M extends RpcMethod>(
  method: M,
  input: RpcInput<M>,
): Promise<RpcOutput<M>> {
  if (_transport == null) {
    throw new Error("RPC not initialized. Call initRpc(transport) first.");
  }
  return _transport(method, input);
}

/**
 * Convenience: creates a fetch-based transport for local development.
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
