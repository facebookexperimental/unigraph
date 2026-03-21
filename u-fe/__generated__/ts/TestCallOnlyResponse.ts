/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { AddOutput } from './AddOutput.ts';
import type { PingOutput } from './PingOutput.ts';

export type TestCallOnlyResponse =
  { "Ping": PingOutput } |
  { "Add": AddOutput };

export type TestCallOnlyResponseVariants = "Ping" | "Add";