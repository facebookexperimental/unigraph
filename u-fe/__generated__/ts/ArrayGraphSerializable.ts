/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


import type { ArrayGraphNodes } from './ArrayGraphNodes.ts';
import type { ArrayGraphSerializableEdges } from './ArrayGraphSerializableEdges.ts';
import type { ArrayGraphSerializableNodeMetadata } from './ArrayGraphSerializableNodeMetadata.ts';
import type { BudgetConfig } from './BudgetConfig.ts';
import type { GraphSettings } from './GraphSettings.ts';
import type { TraversalConfig } from './TraversalConfig.ts';

/**
 * A serializable representation of an array graph, which can be used for
 * storing or transmitting the graph structure.
 * 
 * IMPORTANT: When adding or removing fields here, you MUST update ALL of
 * the following to maintain field parity:
 *   - `ArrayGraph` struct (array_graph.rs)
 *   - `From<ArrayGraph> for ArrayGraphSerializable`
 *   - `From<ArrayGraphSerializable> for ArrayGraph`
 *   - `ArrayGraphSerializable::remap()`
 *   - `ManifestBlobs` struct, `pack()`, and `unpack()` (package.rs)
 *   - `ManifestBlobs::get_all_blob_ids()`
 *   - `apply_deltas()` (delta/apply.rs)
 *   - `remap_with_nodes()` (twin_graph/merge.rs)
 *   - `MapGraph::to_array_graph_serializable()` (map_graph.rs)
 *   - `super_root::append_super_root()` destructure + reconstruction
 */
export interface ArrayGraphSerializable {
  node_names_ordered: ArrayGraphNodes;
  edges: ArrayGraphSerializableEdges;
  node_metadata: ArrayGraphSerializableNodeMetadata;
  graph_settings?: GraphSettings | undefined;
  traversal_config?: TraversalConfig | undefined;
  budget_configs: { [key: string]: BudgetConfig };
  /**
   * If present, these graph will use these entrypoints instead
   * of automatically determining them.
   */
  entry_points?: string[] | undefined;
}