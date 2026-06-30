/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<19424e39ad7c34a57e3ce45134d342cb>>
 */


import type { AddOutput } from './AddOutput.ts';
import type { PingOutput } from './PingOutput.ts';
import type { RpcError } from './RpcError.ts';

export type TestRPCResponse =
  { "Ping": PingOutput } |
  { "Add": AddOutput } |
  { "Error": RpcError };

export type TestRPCResponseVariants = "Ping" | "Add" | "Error";