/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AddOutput } from './AddOutput.ts';
import type { PingOutput } from './PingOutput.ts';

export type TestRPCResponse =
  { "Ping": PingOutput } |
  { "Add": AddOutput };

export type TestRPCResponseVariants = "Ping" | "Add";