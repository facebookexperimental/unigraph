// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { NodeIDX } from "@/__generated__/ts/NodeIDX";

/// Class that contains metrics extracted from the graph on WASM side.
/// WASM<->JS interop is not cheap, so we do these operations in
/// batches and cache the results on JS side so we don't have to go
/// multiple to WASM for the same data.
export class SingleMetricsCache {
  private metrics: Float32Array;
  private valueExists: Uint8Array;
  private getMetrics: (nodeIDXs: NodeIDX[]) => Float32Array;

  constructor(size: number, getMetrics: (nodeIDXs: NodeIDX[]) => Float32Array) {
    this.metrics = new Float32Array(size);
    this.valueExists = new Uint8Array(size).fill(0);
    this.getMetrics = getMetrics;
  }

  getForIDXs(nodeIDXs: NodeIDX[]): Float32Array {
    const cacheMissesIDXs: NodeIDX[] = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      if (this.valueExists[nodeIDX] === 0) {
        cacheMissesIDXs.push(nodeIDX);
      }
    }

    if (cacheMissesIDXs.length !== 0) {
      const newMetrics = this.getMetrics(cacheMissesIDXs);
      for (let i = 0; i < cacheMissesIDXs.length; i++) {
        const nodeIDX = cacheMissesIDXs[i] as NodeIDX;
        const value = newMetrics[i] as number;
        this.metrics[nodeIDX] = value;
        this.valueExists[nodeIDX] = 1;
      }
    }

    const result = new Float32Array(nodeIDXs.length);

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      // at this point there should not be missing values
      result[i] = this.metrics[nodeIDX] as number;
    }

    return result;
  }
}

export type KeyedMetrics = { [metricName: string]: number };
export class KeyedMetricsCache {
  private metrics: Array<KeyedMetrics | null>;
  private getMetrics: (nodeIDXs: NodeIDX[]) => Array<KeyedMetrics>;

  constructor(
    size: number,
    getMetrics: (nodeIDXs: NodeIDX[]) => Array<KeyedMetrics>,
  ) {
    this.metrics = new Array(size).fill(null);
    this.getMetrics = getMetrics;
  }

  getForIDXs(nodeIDXs: NodeIDX[]): Array<KeyedMetrics> {
    const cacheMissesIDXs: NodeIDX[] = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      if (this.metrics[nodeIDX] === null) {
        cacheMissesIDXs.push(nodeIDX);
      }
    }

    if (cacheMissesIDXs.length !== 0) {
      const newMetrics = this.getMetrics(cacheMissesIDXs);
      for (let i = 0; i < cacheMissesIDXs.length; i++) {
        const nodeIDX = cacheMissesIDXs[i] as NodeIDX;
        const value = newMetrics[i] as KeyedMetrics;
        this.metrics[nodeIDX] = value;
      }
    }

    const result = [];

    for (let i = 0; i < nodeIDXs.length; i++) {
      const nodeIDX = nodeIDXs[i] as NodeIDX;
      // at this point there should not be missing values
      result[i] = this.metrics[nodeIDX] as KeyedMetrics;
    }

    return result;
  }
}
