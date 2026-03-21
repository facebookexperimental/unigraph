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

export async function rpc<M extends RpcMethod>(
  method: M,
  input: RpcInput<M>,
): Promise<RpcOutput<M>> {
  const r = await fetch("/api/rpc", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ [method]: input }),
  });
  if (!r.ok) throw new Error(`RPC ${method}: HTTP ${r.status}`);
  const resp = await r.json();
  return resp[method];
}
