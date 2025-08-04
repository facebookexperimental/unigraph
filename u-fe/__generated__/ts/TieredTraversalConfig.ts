import type { AscendingTiersConfig } from "./AscendingTiersConfig.ts";

/**
 * Configuration for tiered traversal, which allows traversing the graph in tiers.
 * Specific use case for this is JavaScript loading tiers. E.g. initial payload vs.
 * lazyloaded JS.
 * When we traverse the graph we look at the tagged edges. If the edge has a tag
 * we look at the node's current tier and then we look at the new tier this node
 * is supposed to transition to and record that.
 */
export type TieredTraversalConfig =
  /** Simple ascending tiers configuration. This is specifically used for JS loading tiers. Certain tagged edges will transition from one tier to another. We can only transition up, not down. e.g., once you LazyLoad (second tier) a JS module, everything past that tier will be considered lazyloaded You can't go back to the initial tier. */
  { AscendingTiers: AscendingTiersConfig };
