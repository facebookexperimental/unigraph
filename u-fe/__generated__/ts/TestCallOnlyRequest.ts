/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<bb5b09046d50921e26abec4654df932f>>
 */


import type { AddInput } from './AddInput.ts';
import type { PingInput } from './PingInput.ts';

export type TestCallOnlyRequest =
  { "Ping": PingInput } |
  { "Add": AddInput };

export type TestCallOnlyRequestVariants = "Ping" | "Add";