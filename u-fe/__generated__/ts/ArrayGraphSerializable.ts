/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */

import type { ArrayGraphNodes } from "./ArrayGraphNodes.ts";
import type { ArrayGraphSerializableEdges } from "./ArrayGraphSerializableEdges.ts";
import type { ArrayGraphSerializableNodeMetadata } from "./ArrayGraphSerializableNodeMetadata.ts";
import type { GraphSettings } from "./GraphSettings.ts";
import type { TraversalConfig } from "./TraversalConfig.ts";

/**
 * A serializable representation of an array graph, which can be used for
 * storing or transmitting the graph structure.
 */
export interface ArrayGraphSerializable {
  node_names_ordered: ArrayGraphNodes;
  edges: ArrayGraphSerializableEdges;
  node_metadata: ArrayGraphSerializableNodeMetadata;
  graph_settings?: GraphSettings | undefined;
  traversal_config?: TraversalConfig | undefined;
  /**
   * If present, these graph will use these entrypoints instead
   * of automatically determining them.
   */
  entry_points?: string[] | undefined;
}
