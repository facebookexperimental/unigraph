// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use anyhow::Result;
use k9::snapshot;
use unigraph_app::GetConfigsInput;
use unigraph_app::PutConfigsInput;
use unigraph_app::call_rpc;
use unigraph_core::Decision;
use unigraph_core::TraversalConfig;
use unigraph_core::config_query::GraphQueryConfig;

use crate::support::app::init_app;

#[tokio::test]
async fn store_and_fetch_configs() -> Result<()> {
    let t = init_app();

    // Store a TVC and a GQC (which embeds the same TVC) via RPC
    let put = call_rpc!(
        t,
        PutConfigs(PutConfigsInput {
            traversal_configs: vec![sample_tvc()],
            graph_query_configs: vec![sample_gqc()],
        })
    );

    snapshot!(
        format!(
            "tvc_key: {}\ngqc_key: {}",
            put.traversal_configs[0], put.graph_query_configs[0]
        ),
        "
tvc_key: tvc_f044e82cdcb5dff6
gqc_key: gqc_728a0dda5b62b9dc
"
    );

    // Fetch them back via RPC
    let get = call_rpc!(
        t,
        GetConfigs(GetConfigsInput {
            traversal_configs: put.traversal_configs,
            graph_query_configs: put.graph_query_configs,
        })
    );

    snapshot!(
        format_tvc(&get.traversal_configs[0]),
        "
force_nodes:
  moduleA: include
  moduleB: exclude
"
    );

    snapshot!(
        format_gqc(&get.graph_query_configs[0]),
        "
roots: root1, root2
handle: my_timeline~42
force_nodes:
  moduleA: include
  moduleB: exclude
"
    );

    Ok(())
}

// ── Formatting helpers ──────────────────────────────────────────

fn format_tvc(tvc: &TraversalConfig) -> String {
    let mut lines = Vec::new();
    if let Some(force_nodes) = &tvc.force_nodes {
        lines.push("force_nodes:".to_string());
        for (name, decision) in force_nodes {
            let action = if decision.include {
                "include"
            } else {
                "exclude"
            };
            lines.push(format!("  {name}: {action}"));
        }
    }
    if let Some(force_tagged) = &tvc.force_tagged {
        lines.push("force_tagged:".to_string());
        for (tag, decision) in force_tagged {
            let action = if decision.include {
                "include"
            } else {
                "exclude"
            };
            lines.push(format!("  {tag}: {action}"));
        }
    }
    lines.join("\n")
}

fn format_gqc(gqc: &GraphQueryConfig) -> String {
    let mut lines = Vec::new();
    if !gqc.roots.is_empty() {
        let roots: Vec<_> = gqc.roots.iter().collect();
        lines.push(format!(
            "roots: {}",
            roots
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(handle) = &gqc.handle {
        lines.push(format!("handle: {handle}"));
    }
    if let Some(tvc) = &gqc.traversal_config {
        let tvc_str = format_tvc(tvc);
        if !tvc_str.is_empty() {
            lines.push(tvc_str);
        }
    }
    lines.join("\n")
}

// ── Fixtures ────────────────────────────────────────────────────

fn sample_tvc() -> TraversalConfig {
    let mut force_nodes = BTreeMap::new();
    force_nodes.insert("moduleA".to_string(), Decision::include());
    force_nodes.insert("moduleB".to_string(), Decision::exclude());
    TraversalConfig {
        force_nodes: Some(force_nodes),
        force_edges: None,
        force_tagged: None,
        label_predicates: None,
        force_dynamic: None,
        tiered_traversal: None,
        messages: None,
    }
}

fn sample_gqc() -> GraphQueryConfig {
    GraphQueryConfig {
        roots: BTreeSet::from(["root1".to_string(), "root2".to_string()]),
        traversal_config: Some(sample_tvc()),
        handle: Some("my_timeline~42".to_string()),
    }
}
