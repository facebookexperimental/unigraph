/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

/** Configuration for size formatting */
export type SizeConfig =
  /** Flexible units to display readable sizes, but will units will be inconsistent across sizes with variation */
  | { VariableUnits: {} }
  /** Forces the units to be in Kilobytes, not to be confused with Kibibytes */
  | { ForcekB: {} }
  /** Forces the units to be in Megabytes, not to be confused with Mebibytes */
  | { ForceMB: {} }
  /** Forces the units to be in Gigabytes, not to be confused with Gibibytes */
  | { ForceGB: {} }
  /**
   * Forces the units to be in Kibibytes. Please consider using ForceKB instead
   * https://fburl.com/workplace/2bl6qcmn
   */
  | { ForceKiB: {} }
  /**
   * Forces the units to be in Mebibytes. Please consider using ForceMB instead
   * https://fburl.com/workplace/2bl6qcmn
   */
  | { ForceMiB: {} }
  /**
   * Forces the units to be in Gigibytes. Please consider using ForceGB instead
   * https://fburl.com/workplace/2bl6qcmn
   */
  | { ForceGiB: {} };

export type SizeConfigVariants =
  | "VariableUnits"
  | "ForcekB"
  | "ForceMB"
  | "ForceGB"
  | "ForceKiB"
  | "ForceMiB"
  | "ForceGiB";
