// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Context;
use anyhow::Result;
use unigraph_core::ArrayGraphSerializable;
use unigraph_core::ArrayGraphSerializableEdges;
use unigraph_core::ArrayGraphSerializableManifest;
use unigraph_core::ArrayGraphSerializableNodeMetadata;
use unigraph_core::ArrayGraphSerializablePackage;
use unigraph_core::ArrayGraphSerializablePackageConfig;
use unigraph_core::BlobID;
use unigraph_core::ManifestBlobs;
use unigraph_core::ManifestStats;

/// Macro to serialize multiple fields to blobs in parallel using Rayon.
///
/// Usage: `into_blobs_parallelized!(field1, field2, field3; all_blobs, config)`
/// Returns a tuple of Vec<BlobID> in the same order.
macro_rules! into_blobs_parallelized {
    ($($field:ident),* ; $all_blobs:expr, $config:expr) => {
        {
            use rayon::scope;
            use std::sync::Mutex;
            use paste::paste;

            // Wrap the all_blobs map in a mutex
            let all_blobs_mutex = Mutex::new($all_blobs);

            paste! {
                // Create individual result variables for each field
                $(
                    let mut [<result_ $field>] = None;
                )*
            }

            scope(|s| {
                $(
                    s.spawn(|_| {
                        let mut temp_blobs = std::collections::BTreeMap::new();
                        let result = unigraph_core::into_blobs(&$field, stringify!($field), &mut temp_blobs, $config)
                            .with_context(|| format!("Failed to serialize field {}", stringify!($field)));

                        paste! {
                            [<result_ $field>] = Some(result);
                        }
                        // Merge temp_blobs into the shared all_blobs map
                        let mut all_blobs_guard = all_blobs_mutex.lock().unwrap();
                        all_blobs_guard.extend(temp_blobs);
                    });
                )*
            });

            // Return tuple with all results in order
            paste! {
                ($(
                    [<result_ $field>].context("Empty value")??,
                )*)
            }
        }
    };
}

/// Converts an `ArrayGraphSerializable` into a manifest and a collection of blobs
/// that can be stored separately and later reconstructed using `from_blobs`.
pub fn pack_parallel(
    graph: &ArrayGraphSerializable,
    c: &ArrayGraphSerializablePackageConfig,
) -> Result<ArrayGraphSerializablePackage> {
    let mut b = BTreeMap::new();

    let ArrayGraphSerializable {
        node_names_ordered,
        edges,
        node_metadata,
        graph_settings,
        traversal_config,
        entry_points,
    } = &graph;

    let ArrayGraphSerializableEdges {
        directed,
        directed_offsets,
        tagged,
        dynamic,
    } = &edges;

    let ArrayGraphSerializableNodeMetadata { metrics, tag_sets } = &node_metadata;

    let (node_names, node_names_offsets) = node_names_ordered.as_parts();

    let (
        node_names,
        node_names_offsets,
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points,
    ) = into_blobs_parallelized!(
        node_names,
        node_names_offsets,
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points;
        &mut b, c
    );

    let manifest_blobs = ManifestBlobs {
        node_names,
        node_names_offsets,
        directed,
        directed_offsets,
        tagged,
        dynamic,
        metrics,
        tag_sets,
        traversal_config,
        entry_points,
    };

    let mut manifest_blob_id = BlobID::from("_manifest.json");

    if let Some(f) = c.modify_blob_id.as_ref() {
        manifest_blob_id = f(&manifest_blob_id.0);
    }

    let stats = ManifestStats::from_blobs(&b, graph);

    let manifest = ArrayGraphSerializableManifest {
        self_reference: manifest_blob_id.clone(),
        stats,
        blobs: manifest_blobs,
        graph_settings: graph_settings.clone(),
    };

    b.insert(
        manifest_blob_id,
        serde_json::to_string_pretty(&manifest)?.into_bytes(),
    );

    Ok(ArrayGraphSerializablePackage { manifest, blobs: b })
}

#[cfg(test)]
mod tests {
    use k9::MultilineString;
    use k9::assert_equal;
    use unigraph_core::MapGraph;

    use super::*;

    // Larger graph with more complex cases. Cycles, multiple parents, etc
    // https://fburl.com/excalidraw/23gavkrb
    const TEST_GRAPH_2: &str =
        include_str!("../../unigraph_core/src/tests/test_graphs/test_graph_2_left.json");

    #[test]
    fn test_parallel_packing() -> Result<()> {
        let ag = MapGraph::from_json(TEST_GRAPH_2)?.to_array_graph_serializable()?;
        let package = pack_parallel(
            &ag,
            &ArrayGraphSerializablePackageConfig {
                bytes_per_blob_chunk: Some(10),
                compression_level: None,
                modify_blob_id: None,
            },
        )?;

        let package_base64 = package.into_base_64();

        let roundtrip = ArrayGraphSerializable::unpack(
            &ArrayGraphSerializablePackage::from_base64(package_base64)?,
        )?;

        let original_str = ag.into_array_graph().debug().to_forward_edges_string()?;
        let roundtrip_str = roundtrip
            .into_array_graph()
            .debug()
            .to_forward_edges_string()?;

        assert_equal!(
            MultilineString(original_str),
            MultilineString(roundtrip_str)
        );
        Ok(())
    }
}
