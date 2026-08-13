/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<1be309b20ba73ab0702d7b4fd34228b5>>
 */


export interface NodeHistory {
  node_name: string;
  /**
   * The node's whole series, delta-encoded and flattened into
   * `4 + metrics.len()`-sized chunks in ascending time order. See the module
   * docs for the layout, and [`decode_series`] for how to read it.
   * 
   * Not indexable on its own: slot `n` means nothing without the stride, and
   * no slot after a column's first is an absolute value.
   */
  deltas: (number | undefined)[];
}