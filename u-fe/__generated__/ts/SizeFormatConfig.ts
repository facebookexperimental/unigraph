/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<fff25ea34fa5c5037ee4bb5ed7857c54>>
 */


import type { SizeInputUnits } from './SizeInputUnits.ts';
import type { SizeOutputUnits } from './SizeOutputUnits.ts';

export interface SizeFormatConfig {
  /** What is the unit of the input value that will be formatted */
  input_units: SizeInputUnits;
  /** Configures the unit format for the size metric, units can be variable or forced (kB/MB/GB) */
  output_units: SizeOutputUnits;
  min_precision?: number | undefined;
  max_precision?: number | undefined;
  use_delimiter?: boolean | undefined;
}