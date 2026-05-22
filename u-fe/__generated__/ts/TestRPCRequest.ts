/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AddInput } from './AddInput.ts';
import type { PingInput } from './PingInput.ts';

export type TestRPCRequest =
  { "Ping": PingInput } |
  { "Add": AddInput };

export type TestRPCRequestVariants = "Ping" | "Add";