/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AddInput } from './AddInput.ts';
import type { PingInput } from './PingInput.ts';

export type TestCallOnlyRequest =
  { "Ping": PingInput } |
  { "Add": AddInput };

export type TestCallOnlyRequestVariants = "Ping" | "Add";