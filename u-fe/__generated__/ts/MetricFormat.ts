/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<8a79917647d7d17df3d7458740694ecd>>
 */


import type { SizeFormatConfig } from './SizeFormatConfig.ts';
import type { TimespanUnits } from './TimespanUnits.ts';

/**
 * Value that defines how to format metric values (in the UI or CLI output)
 * This value is cross platform enum type which is represented as an object/shape
 * with all keys/properties optional and expected to have exactly ONE key/property
 * to be set at runtime.
 */
export type MetricFormat =
  /** a value representing a percentage value. */
  { "Percent": { scaled_percentage: boolean | undefined } } |
  /** Given a value of bytes, format it as a size (e.g. 1.4MB, 2kB, etc) */
  { "Size": SizeFormatConfig } |
  /** Given a value of 0 or 1, format it as a boolean */
  { "NumericBoolean": {  } } |
  /**
   * 1       -> {min:    2, max: 4, delimiter: true}  -> "1.00"
   * 1.1     -> {min:    2, max: 4, delimiter: true}  -> "1.10"
   * 1.12    -> {min:    2, max: 4, delimiter: true}  -> "1.12"
   * 1.123   -> {min:    2, max: 4, delimiter: true}  -> "1.123"
   * 1.1234  -> {min:    2, max: 4, delimiter: true}  -> "1.1234"
   * 1.12345 -> {min:    2, max: 4, delimiter: true}  -> "1.1235"
   * 1000000 -> {min:    2, max: 4, delimiter: true}  -> "1,000,000.00"
   * 1000000 -> {min:    2, max: 4, delimiter: false} -> "1000000.00"
   * 1000000 -> {min:    0, max: 0, delimiter: true}  -> "1,000,000"
   */
  { "NumberWithVariablePrecision": { min_precision: number | undefined, max_precision: number | undefined, use_delimiter: boolean | undefined } } |
  /**
   * Treat the value as an enum: map an integer value to a display label.
   * The metric value is coerced to an integer (rounded) before lookup.
   * e.g. {0 => "root", 1 => "nested", 3 => "bootload"}
   * Values without a matching label fall back to the raw integer string.
   */
  { "Enum": { variants: { [key: number]: string } } } |
  /**
   * Marks a metric as the START of a timespan (for a tracing/gantt bar).
   * The paired END value lives in a separate metric named by
   * `timespan_end_metric_name`. The UI renders a positioned bar spanning
   * start→end; the CLI and any text context render the raw numeric value.
   */
  { "TimespanStart": { timespan_end_metric_name: string | undefined, units: TimespanUnits, ignore_zero: boolean | undefined } };

export type MetricFormatVariants = "Percent" | "Size" | "NumericBoolean" | "NumberWithVariablePrecision" | "Enum" | "TimespanStart";