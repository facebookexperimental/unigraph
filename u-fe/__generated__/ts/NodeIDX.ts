/**
 * Why is NodeIDX a u32?
 * We pass this across the WASM boundary in batch (as Vec<u64>) where
 * the NodeIDX is packed together with other data.
 * This needs NodeIDX to be consistent across all platforms. Since WASM
 * is always 32-bit we use u32 even on native platforms with usize == u64.
 * There's technically no runtime overhead of doing this and it also saves
 * memory.
 * 
 * Do we actually need 64-bit indices?
 * if this software is able to scale to graph with 18,446,744,073,709,551,615 nodes
 * i will personally rewrite the entire codebase to support u64.
 */
export type NodeIDX = number;