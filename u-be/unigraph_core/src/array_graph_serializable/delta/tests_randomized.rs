// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Randomized round-trip tests for the delta system.
//!
//! All tests use deterministic seeds via XorShift64 PRNG, so failures are
//! reproducible. The graph generator produces graphs with directed, tagged,
//! and dynamic edges, metrics, tag sets, settings, traversal configs,
//! and entry points.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use anyhow::Result;
    use unigraph_delta::Deltable;

    use crate::ArrayGraphDynamicEdge;
    use crate::ArrayGraphNodes;
    use crate::ArrayGraphSerializable;
    use crate::ArrayGraphSerializableNodeMetadata;
    use crate::NodeIDX;
    use crate::TraversalConfig;
    use crate::array_graph_serializable::delta::apply_delta;
    use crate::array_graph_serializable::delta::apply_deltas;
    use crate::array_graph_serializable::delta::derive_delta;
    use crate::array_graph_serializable::delta::package::pack_delta;
    use crate::array_graph_serializable::delta::package::unpack_delta;
    use crate::array_graph_serializable::package::ArrayGraphSerializablePackageConfig;
    use crate::graph_settings::ArrayGraphUISettings;
    use crate::graph_settings::ColumnSettings;
    use crate::graph_settings::GraphSettings;
    use crate::graph_settings::GraphStructure;
    use crate::graph_settings::SidebarPanel;
    use crate::traversal::Decision;

    // -----------------------------------------------------------------------
    // XorShift64 PRNG
    // -----------------------------------------------------------------------

    struct XorShift64 {
        state: u64,
    }

    impl XorShift64 {
        fn new(seed: u64) -> Self {
            Self {
                state: if seed == 0 { 1 } else { seed },
            }
        }

        fn next(&mut self) -> u64 {
            let mut x = self.state;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.state = x;
            x
        }

        fn next_bool(&mut self, pct_chance: u64) -> bool {
            self.next() % 100 < pct_chance
        }

        fn next_f32(&mut self) -> f32 {
            (self.next() % 10000) as f32 / 100.0
        }

        fn pick<'a>(&mut self, items: &'a [&str]) -> &'a str {
            items[(self.next() % items.len() as u64) as usize]
        }
    }

    // -----------------------------------------------------------------------
    // Random graph generator
    // -----------------------------------------------------------------------

    fn random_graph(seed: u64) -> ArrayGraphSerializable {
        let mut rng = XorShift64::new(seed.wrapping_mul(6364136223846793005).wrapping_add(1));

        // 3-30 nodes
        let node_count = 3 + (rng.next() % 28) as usize;

        let mut node_names_str = String::new();
        let mut node_name_offsets = vec![0usize];
        for i in 0..node_count {
            let name = format!("n_{:03}", i);
            node_names_str.push_str(&name);
            node_name_offsets.push(node_names_str.len());
        }

        // Directed edges: 0-4 per node (deduplicated via BTreeSet)
        let mut directed = Vec::new();
        let mut directed_offsets = vec![0usize];
        for _src in 0..node_count {
            let edge_count = rng.next() % 5;
            let mut targets = BTreeSet::new();
            for _ in 0..edge_count {
                let target = (rng.next() % node_count as u64) as usize;
                targets.insert(NodeIDX::from(target));
            }
            for target in targets {
                directed.push(target);
            }
            directed_offsets.push(directed.len());
        }

        // Metrics: 3 metrics
        let mut metrics = BTreeMap::new();
        for m in 0..3 {
            let values: Vec<f32> = (0..node_count).map(|_| rng.next_f32()).collect();
            metrics.insert(format!("metric_{}", m), values);
        }

        // Tagged edges: ~30% of nodes
        let mut tagged: BTreeMap<NodeIDX, BTreeMap<String, BTreeSet<NodeIDX>>> = BTreeMap::new();
        for src in 0..node_count {
            if rng.next_bool(30) {
                let tag = format!("tag_{}", rng.next() % 4);
                let target = NodeIDX::from((rng.next() % node_count as u64) as usize);
                tagged
                    .entry(NodeIDX::from(src))
                    .or_default()
                    .entry(tag)
                    .or_default()
                    .insert(target);
            }
        }

        // Dynamic edges: ~15% of nodes
        let mut dynamic: BTreeMap<
            NodeIDX,
            BTreeMap<String, BTreeMap<String, ArrayGraphDynamicEdge>>,
        > = BTreeMap::new();
        for src in 0..node_count {
            if rng.next_bool(15) {
                let type_key = format!("dtype_{}", rng.next() % 3);
                let edge_name = format!("dedge_{}", rng.next() % 5);
                let branch_name = format!("branch_{}", rng.next() % 3);
                let target = NodeIDX::from((rng.next() % node_count as u64) as usize);
                let edge = ArrayGraphDynamicEdge {
                    branches: BTreeMap::from([(branch_name, BTreeSet::from([target]))]),
                    metadata: if rng.next_bool(50) {
                        Some(BTreeMap::from([(
                            "key".to_string(),
                            format!("val_{}", rng.next() % 10),
                        )]))
                    } else {
                        None
                    },
                };
                dynamic
                    .entry(NodeIDX::from(src))
                    .or_default()
                    .entry(type_key)
                    .or_default()
                    .insert(edge_name, edge);
            }
        }

        // Labels: ~20% of nodes (inverted index: label_name → node → values)
        let mut labels: BTreeMap<String, BTreeMap<NodeIDX, BTreeSet<String>>> = BTreeMap::new();
        for node in 0..node_count {
            if rng.next_bool(20) {
                let label_name = format!("set_{}", rng.next() % 3);
                let label_value = format!("val_{}", rng.next() % 10);
                labels
                    .entry(label_name)
                    .or_default()
                    .entry(NodeIDX::from(node))
                    .or_default()
                    .insert(label_value);
            }
        }

        // Properties: ~15% of nodes (inverted index: prop_name → node → value)
        let mut properties: BTreeMap<String, BTreeMap<NodeIDX, String>> = BTreeMap::new();
        for node in 0..node_count {
            if rng.next_bool(15) {
                let prop_name = format!("prop_{}", rng.next() % 3);
                let prop_value = format!("pval_{}", rng.next() % 10);
                properties
                    .entry(prop_name)
                    .or_default()
                    .insert(NodeIDX::from(node), prop_value);
            }
        }

        // Optional graph settings (~30% chance)
        let graph_settings = if rng.next_bool(30) {
            Some(random_graph_settings(&mut rng))
        } else {
            None
        };

        // Optional traversal config (~25% chance)
        let traversal_config = if rng.next_bool(25) {
            Some(random_traversal_config(&mut rng))
        } else {
            None
        };

        // Optional entry points (~20% chance)
        let entry_points = if rng.next_bool(20) {
            let count = 1 + (rng.next() % 3) as usize;
            let ep: BTreeSet<String> = (0..count)
                .map(|_| format!("n_{:03}", rng.next() % node_count as u64))
                .collect();
            Some(ep)
        } else {
            None
        };

        // Build CSR using the same pipeline as production (MapGraph → AGS)
        // First build the directed CSR + old tagged/dynamic, then convert to unified CSR
        let edges = build_unified_csr(&directed, &directed_offsets, &tagged, &dynamic, node_count);
        ArrayGraphSerializable {
            node_names_ordered: ArrayGraphNodes::from_parts(node_names_str, node_name_offsets),
            edges,
            node_metadata: ArrayGraphSerializableNodeMetadata {
                metrics,
                labels,
                properties,
            },
            graph_settings,
            traversal_config,
            entry_points,
            properties: random_graph_properties(&mut rng),
        }
    }

    fn random_graph_settings(rng: &mut XorShift64) -> GraphSettings {
        GraphSettings {
            description: if rng.next_bool(30) {
                Some(format!("graph-desc-{}", rng.next() % 1000))
            } else {
                None
            },
            ui_settings: if rng.next_bool(70) {
                Some(ArrayGraphUISettings {
                    selected_sidebar_panel: if rng.next_bool(50) {
                        Some(match rng.next() % 3 {
                            0 => SidebarPanel::None,
                            1 => SidebarPanel::Simulation,
                            _ => SidebarPanel::GraphInfo,
                        })
                    } else {
                        None
                    },
                    columns: if rng.next_bool(40) {
                        Some(ColumnSettings {
                            hide_metrics: if rng.next_bool(50) {
                                Some(rng.next_bool(50))
                            } else {
                                None
                            },
                            show_counts: if rng.next_bool(50) {
                                Some(rng.next_bool(50))
                            } else {
                                None
                            },
                            show_tier_column: if rng.next_bool(30) {
                                Some(rng.next_bool(50))
                            } else {
                                None
                            },
                            ..Default::default()
                        })
                    } else {
                        None
                    },
                    graph_structure: if rng.next_bool(40) {
                        Some(match rng.next() % 3 {
                            0 => GraphStructure::Forward,
                            1 => GraphStructure::Dominator,
                            _ => GraphStructure::Reverse,
                        })
                    } else {
                        None
                    },
                    show_changed_nodes_only: None,
                    entry_points: None,
                    entry_points_specified: None,
                })
            } else {
                None
            },
        }
    }

    fn random_traversal_config(rng: &mut XorShift64) -> TraversalConfig {
        let node_names = ["n_000", "n_001", "n_002", "n_003", "n_004"];
        let tag_names = ["lazy", "async", "sync", "eager"];

        TraversalConfig {
            force_nodes: if rng.next_bool(50) {
                let count = 1 + (rng.next() % 3) as usize;
                let nodes: BTreeMap<String, Decision> = (0..count)
                    .map(|_| {
                        let name = rng.pick(&node_names).to_string();
                        let decision = if rng.next_bool(50) {
                            Decision::include()
                        } else {
                            Decision::exclude()
                        };
                        (name, decision)
                    })
                    .collect();
                Some(nodes)
            } else {
                None
            },
            force_tagged: if rng.next_bool(40) {
                let count = 1 + (rng.next() % 2) as usize;
                let tags: BTreeMap<String, Decision> = (0..count)
                    .map(|_| {
                        let tag = rng.pick(&tag_names).to_string();
                        let decision = if rng.next_bool(50) {
                            Decision::include()
                        } else {
                            Decision::exclude()
                        };
                        (tag, decision)
                    })
                    .collect();
                Some(tags)
            } else {
                None
            },
            force_edges: None,
            label_predicates: None,
            force_dynamic: None,
            tiered_traversal: None,
            messages: None,
        }
    }

    /// Random graph-level properties (~30% chance, 1-3 key-value pairs)
    fn random_graph_properties(rng: &mut XorShift64) -> BTreeMap<String, String> {
        if rng.next_bool(30) {
            let count = 1 + (rng.next() % 3) as usize;
            (0..count)
                .map(|_| {
                    (
                        format!("gprop_{}", rng.next() % 5),
                        format!("gval_{}", rng.next() % 10),
                    )
                })
                .collect()
        } else {
            BTreeMap::new()
        }
    }

    // -----------------------------------------------------------------------
    // Integrity hash
    // -----------------------------------------------------------------------

    fn integrity_string(graph: &ArrayGraphSerializable) -> String {
        let json = serde_json::to_string(graph).unwrap();
        let hash = xxhash_rust::xxh3::xxh3_64(json.as_bytes());
        format!("{:016x}", hash)
    }

    fn graphs_equal(a: &ArrayGraphSerializable, b: &ArrayGraphSerializable) -> bool {
        serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn randomized_single_delta_roundtrip() -> Result<()> {
        for i in 0..100 {
            let base = random_graph(i * 2);
            let target = random_graph(i * 2 + 1);
            let delta = derive_delta(&base, &target)?;
            let result = apply_delta(base, &delta)?;
            assert!(
                graphs_equal(&result, &target),
                "Round-trip failed for pair {}",
                i
            );
        }
        Ok(())
    }

    #[test]
    fn randomized_delta_chain() -> Result<()> {
        let mut current = random_graph(1000);
        for step in 0..50 {
            let next = random_graph(1000 + step + 1);
            let delta = derive_delta(&current, &next)?;
            current = apply_delta(current, &delta)?;
            assert!(graphs_equal(&current, &next), "Chain step {} failed", step);
        }
        Ok(())
    }

    #[test]
    fn randomized_batch_vs_sequential() -> Result<()> {
        let base_seed = 2000u64;

        // Generate 20 graphs (base + 20 steps)
        let seeds: Vec<u64> = (0..=20).map(|i| base_seed + i).collect();

        // Derive sequential deltas between adjacent pairs
        let mut deltas = Vec::new();
        for i in 0..20 {
            let from = random_graph(seeds[i]);
            let to = random_graph(seeds[i + 1]);
            deltas.push(derive_delta(&from, &to)?);
        }

        // Sequential application
        let base = random_graph(seeds[0]);
        let mut sequential = base;
        for d in &deltas {
            sequential = apply_delta(sequential, d)?;
        }

        // Batch application
        let base = random_graph(seeds[0]);
        let batch = apply_deltas(base, &deltas)?;

        assert!(
            graphs_equal(&sequential, &batch),
            "Batch vs sequential mismatch"
        );
        Ok(())
    }

    #[test]
    fn randomized_settings_delta_roundtrip() -> Result<()> {
        for i in 0..50 {
            let mut rng = XorShift64::new(i * 7 + 3000);
            let base = if rng.next_bool(70) {
                random_graph_settings(&mut rng)
            } else {
                GraphSettings::default()
            };
            let target = if rng.next_bool(70) {
                random_graph_settings(&mut rng)
            } else {
                GraphSettings::default()
            };

            if base != target {
                let delta = base.derive_delta(&target).unwrap();
                let mut result = base;
                result.apply_delta(delta).unwrap();
                assert_eq!(result, target, "Settings round-trip failed for pair {}", i);
            }
        }
        Ok(())
    }

    #[test]
    fn randomized_traversal_config_delta_roundtrip() -> Result<()> {
        for i in 0..50 {
            let mut rng_base = XorShift64::new(i * 11 + 4000);
            let base = random_traversal_config(&mut rng_base);
            let target = random_traversal_config(&mut rng_base);

            if base != target {
                let delta = base.derive_delta(&target).unwrap();
                let mut result = base;
                result.apply_delta(delta).unwrap();
                assert_eq!(
                    result, target,
                    "TraversalConfig round-trip failed for pair {}",
                    i
                );
            }
        }
        Ok(())
    }

    #[test]
    fn randomized_empty_delta_identity() -> Result<()> {
        for i in 0..100 {
            let graph = random_graph(5000 + i);
            let delta = derive_delta(&graph, &graph)?;
            assert!(
                delta.is_empty(),
                "Identity delta was not empty for graph {}",
                i
            );
        }
        Ok(())
    }

    #[test]
    fn randomized_pack_unpack_roundtrip() -> Result<()> {
        for i in 0..50 {
            let base = random_graph(6000 + i * 2);
            let target = random_graph(6000 + i * 2 + 1);
            let delta = derive_delta(&base, &target)?;

            let package = pack_delta(&delta, &ArrayGraphSerializablePackageConfig::default())?;
            let unpacked = unpack_delta(&package, &ll::Task::create_new("test"))?;

            // Apply both and compare
            let from_original = apply_delta(random_graph(6000 + i * 2), &delta)?;
            let from_unpacked = apply_delta(random_graph(6000 + i * 2), &unpacked)?;
            assert!(
                graphs_equal(&from_original, &from_unpacked),
                "Pack/unpack round-trip failed for pair {}",
                i
            );
        }
        Ok(())
    }

    #[test]
    fn randomized_integrity_snapshot() -> Result<()> {
        // Generate 10 deterministic graphs and hash their delta round-trips
        // to catch any unintentional behavior changes
        let mut hashes = Vec::new();
        for i in 0..10 {
            let base = random_graph(7000 + i * 2);
            let target = random_graph(7000 + i * 2 + 1);
            let delta = derive_delta(&base, &target)?;
            let result = apply_delta(base, &delta)?;
            hashes.push(format!("pair_{:02}: {}", i, integrity_string(&result)));
        }
        k9::snapshot!(
            hashes.join("\n"),
            "
pair_00: 41c61b7d86012725
pair_01: 9c229d465a595694
pair_02: 02c7db2854a102fc
pair_03: 8acc05ff083d6578
pair_04: 1d449b7e40ad8e9b
pair_05: 92245372e4878666
pair_06: 9c012d55a738aa83
pair_07: 540e7ca607fc602b
pair_08: ffad51a1d237856e
pair_09: 8f5806421a6b0867
"
        );
        Ok(())
    }

    /// Build a unified CSR from separate directed/tagged/dynamic edge structures.
    fn build_unified_csr(
        directed: &[crate::NodeIDX],
        directed_offsets: &[usize],
        tagged: &std::collections::BTreeMap<
            crate::NodeIDX,
            std::collections::BTreeMap<String, std::collections::BTreeSet<crate::NodeIDX>>,
        >,
        dynamic: &std::collections::BTreeMap<
            crate::NodeIDX,
            std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<String, crate::ArrayGraphDynamicEdge>,
            >,
        >,
        node_count: usize,
    ) -> crate::ArrayGraphSerializableEdges {
        use std::collections::BTreeMap;

        use crate::ArrayGraphSerializableEdges;
        use crate::EdgeIDX;
        use crate::EdgeMeta;
        use crate::EdgeMetaIDX;
        use crate::NodeIDX;

        let mut edges = Vec::new();
        let mut edge_offsets = Vec::with_capacity(node_count + 1);
        edge_offsets.push(0);
        let mut edge_metadata: Vec<EdgeMeta> = Vec::new();
        let mut edge_metadata_map: BTreeMap<EdgeIDX, EdgeMetaIDX> = BTreeMap::new();

        for i in 0..node_count {
            let node_idx = NodeIDX::from(i);
            let start = directed_offsets[i];
            let end = directed_offsets[i + 1];
            edges.extend_from_slice(&directed[start..end]);

            if let Some(tag_map) = tagged.get(&node_idx) {
                for (tag, targets) in tag_map {
                    let meta_idx = EdgeMetaIDX::from(edge_metadata.len());
                    edge_metadata.push(EdgeMeta::Tagged { tag: tag.clone() });
                    for &target in targets {
                        let edge_idx = EdgeIDX::from(edges.len());
                        edges.push(target);
                        edge_metadata_map.insert(edge_idx, meta_idx);
                    }
                }
            }

            if let Some(type_map) = dynamic.get(&node_idx) {
                for (type_key, edge_map) in type_map {
                    for (edge_name, dyn_edge) in edge_map {
                        for (branch, targets) in &dyn_edge.branches {
                            let meta_idx = EdgeMetaIDX::from(edge_metadata.len());
                            edge_metadata.push(EdgeMeta::Dynamic {
                                type_key: type_key.clone(),
                                edge_name: edge_name.clone(),
                                branch: branch.clone(),
                                metadata: dyn_edge.metadata.clone(),
                            });
                            for &target in targets {
                                let edge_idx = EdgeIDX::from(edges.len());
                                edges.push(target);
                                edge_metadata_map.insert(edge_idx, meta_idx);
                            }
                        }
                    }
                }
            }

            edge_offsets.push(edges.len());
        }

        ArrayGraphSerializableEdges {
            edges,
            edge_offsets,
            edge_metadata,
            edge_metadata_map,
        }
    }
}
