/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AddOutput } from './AddOutput.ts';
import type { PingOutput } from './PingOutput.ts';
import type { RpcError } from './RpcError.ts';

export type TestCallOnlyResponse =
  { "Ping": PingOutput } |
  { "Add": AddOutput } |
  { "Error": RpcError };

export type TestCallOnlyResponseVariants = "Ping" | "Add" | "Error";