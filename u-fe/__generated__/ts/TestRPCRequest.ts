/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<ff5949235e7f82d11fc1f7ab9580a459>>
 */


import type { AddInput } from './AddInput.ts';
import type { PingInput } from './PingInput.ts';

export type TestRPCRequest =
  { "Ping": PingInput } |
  { "Add": AddInput };

export type TestRPCRequestVariants = "Ping" | "Add";