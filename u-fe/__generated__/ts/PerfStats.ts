/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<4d394b4f642352cabb264f1676c417fc>>
 */


/**
 * Where a request's time went. Every field is prefixed by the stage it
 * measures, so new stages can be added alongside these without renaming them.
 */
export interface PerfStats {
  /** True when the graph came out of the LRU without touching storage. */
  cache_hit: boolean;
  /**
   * Wall time the caller spent inside the cache lookup.
   * 
   * This includes time spent blocked behind another caller's in-flight fetch
   * for the same key, so a hit is not automatically fast — a slow hit means
   * the entry was being computed by someone else while this caller waited.
   */
  cache_elapsed_ms: number;
}