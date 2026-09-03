/**
 * Integer constants: unquoted in every target language, and a Hack `int`
 * enum rather than a `string` one.
 */
export const Thresholds = {
  /** Smallest change worth reporting, in bytes. */
  SIGNIFICANT_BYTES: 1000,
  ZERO: 0,
  /**
   * The largest value a JS `number` holds exactly. One more than this is
   * a compile error, because it would round in the Flow and TS output.
   */
  MAX_SAFE: 9007199254740991,
} as const;

export type Thresholds = (typeof Thresholds)[keyof typeof Thresholds];
