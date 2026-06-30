/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f6ef3dfb2eb2971fde8b25db53e8c4a5>>
 */


import type { AddOutput } from './AddOutput.ts';
import type { PingOutput } from './PingOutput.ts';
import type { RpcError } from './RpcError.ts';

export type TestCallOnlyResponse =
  { "Ping": PingOutput } |
  { "Add": AddOutput } |
  { "Error": RpcError };

export type TestCallOnlyResponseVariants = "Ping" | "Add" | "Error";