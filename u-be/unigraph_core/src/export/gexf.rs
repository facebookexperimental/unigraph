// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Gephi Graph Exchange XML Format (GEXF 1.3) writer for [`MapGraph`].
//!
//! Nodes become `<node>` entries (id = enumeration index, label = node name).
//! Per-node metrics/labels/properties become node `<attribute>`s (metrics are
//! `float`, labels/properties are `string`). Directed, tagged, and dynamic
//! edges all become `<edge>`s; tagged/dynamic edges carry a `label` describing
//! their kind. Everything user-supplied is XML-escaped.
//!
//! Purely iterative string building — safe for the very deep graphs unigraph
//! deals with.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write;

use crate::MapGraph;

pub fn map_graph_to_gexf(graph: &MapGraph) -> String {
    let node_ids = assign_node_ids(graph);
    let attrs = collect_node_attributes(graph);

    let mut out = String::new();
    write_header(&mut out);
    write_attribute_defs(&mut out, &attrs);
    write_nodes(&mut out, graph, &node_ids, &attrs);
    write_edges(&mut out, graph, &node_ids);
    write_footer(&mut out);
    out
}

/// Stable integer ids for the graph's attribute columns, split by kind. Ids are
/// allocated across all kinds in one sequence (metrics, then labels, then
/// properties) so a single `<attributes>` block can declare them all.
struct NodeAttributes<'a> {
    metric_ids: BTreeMap<&'a str, usize>,
    label_ids: BTreeMap<&'a str, usize>,
    property_ids: BTreeMap<&'a str, usize>,
}

fn assign_node_ids(graph: &MapGraph) -> BTreeMap<&str, usize> {
    graph
        .nodes
        .keys()
        .enumerate()
        .map(|(id, name)| (name.as_str(), id))
        .collect()
}

fn collect_node_attributes(graph: &MapGraph) -> NodeAttributes<'_> {
    let mut metrics: BTreeSet<&str> = BTreeSet::new();
    let mut labels: BTreeSet<&str> = BTreeSet::new();
    let mut properties: BTreeSet<&str> = BTreeSet::new();

    for node in graph.nodes.values() {
        for name in node.metrics.iter().flatten().map(|(k, _)| k.as_str()) {
            metrics.insert(name);
        }
        for name in node.labels.iter().flatten().map(|(k, _)| k.as_str()) {
            labels.insert(name);
        }
        for name in node.properties.iter().flatten().map(|(k, _)| k.as_str()) {
            properties.insert(name);
        }
    }

    let mut next_id = 0;
    NodeAttributes {
        metric_ids: assign_ids(metrics, &mut next_id),
        label_ids: assign_ids(labels, &mut next_id),
        property_ids: assign_ids(properties, &mut next_id),
    }
}

/// Allocate sequential ids for a set of attribute names, advancing `next_id`.
fn assign_ids<'a>(names: BTreeSet<&'a str>, next_id: &mut usize) -> BTreeMap<&'a str, usize> {
    names
        .into_iter()
        .map(|name| {
            let id = *next_id;
            *next_id += 1;
            (name, id)
        })
        .collect()
}

fn write_header(out: &mut String) {
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<gexf xmlns=\"http://gexf.net/1.3\" version=\"1.3\">\n");
    out.push_str("<meta>\n<creator>unigraph</creator>\n</meta>\n");
    out.push_str("<graph mode=\"static\" defaultedgetype=\"directed\">\n");
}

fn write_footer(out: &mut String) {
    out.push_str("</graph>\n</gexf>\n");
}

fn write_attribute_defs(out: &mut String, attrs: &NodeAttributes<'_>) {
    if attrs.metric_ids.is_empty() && attrs.label_ids.is_empty() && attrs.property_ids.is_empty() {
        return;
    }
    out.push_str("<attributes class=\"node\">\n");
    write_attribute_defs_of_type(out, &attrs.metric_ids, "float");
    write_attribute_defs_of_type(out, &attrs.label_ids, "string");
    write_attribute_defs_of_type(out, &attrs.property_ids, "string");
    out.push_str("</attributes>\n");
}

fn write_attribute_defs_of_type(out: &mut String, ids: &BTreeMap<&str, usize>, ty: &str) {
    // Emit in id order so the declarations read top-to-bottom by id.
    let mut ordered: Vec<(&&str, &usize)> = ids.iter().collect();
    ordered.sort_by_key(|(_, id)| **id);
    for (title, id) in ordered {
        let _ = write!(
            out,
            "<attribute id=\"{}\" title=\"{}\" type=\"{}\"/>\n",
            id,
            xml_escape(title),
            ty
        );
    }
}

fn write_nodes(
    out: &mut String,
    graph: &MapGraph,
    node_ids: &BTreeMap<&str, usize>,
    attrs: &NodeAttributes<'_>,
) {
    out.push_str("<nodes>\n");
    for (name, node) in &graph.nodes {
        let id = node_ids[name.as_str()];
        let _ = write!(out, "<node id=\"{}\" label=\"{}\">\n", id, xml_escape(name));
        write_node_attvalues(out, node, attrs);
        out.push_str("</node>\n");
    }
    out.push_str("</nodes>\n");
}

fn write_node_attvalues(out: &mut String, node: &crate::GraphNode, attrs: &NodeAttributes<'_>) {
    let has_any = node.metrics.as_ref().is_some_and(|m| !m.is_empty())
        || node.labels.as_ref().is_some_and(|l| !l.is_empty())
        || node.properties.as_ref().is_some_and(|p| !p.is_empty());
    if !has_any {
        return;
    }

    out.push_str("<attvalues>\n");
    for (name, value) in node.metrics.iter().flatten() {
        let id = attrs.metric_ids[name.as_str()];
        let _ = write!(out, "<attvalue for=\"{}\" value=\"{}\"/>\n", id, value);
    }
    for (name, values) in node.labels.iter().flatten() {
        let id = attrs.label_ids[name.as_str()];
        let joined = values
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let _ = write!(
            out,
            "<attvalue for=\"{}\" value=\"{}\"/>\n",
            id,
            xml_escape(&joined)
        );
    }
    for (name, value) in node.properties.iter().flatten() {
        let id = attrs.property_ids[name.as_str()];
        let _ = write!(
            out,
            "<attvalue for=\"{}\" value=\"{}\"/>\n",
            id,
            xml_escape(value)
        );
    }
    out.push_str("</attvalues>\n");
}

fn write_edges(out: &mut String, graph: &MapGraph, node_ids: &BTreeMap<&str, usize>) {
    out.push_str("<edges>\n");
    let mut edge_id = 0;
    for (source_name, node) in &graph.nodes {
        let source = node_ids[source_name.as_str()];

        for target in node.edges_directed.iter().flatten() {
            write_edge(out, &mut edge_id, source, target, None, node_ids);
        }
        for (tag, targets) in node.edges_tagged.iter().flatten() {
            for target in targets {
                write_edge(out, &mut edge_id, source, target, Some(tag), node_ids);
            }
        }
        for (type_key, edges) in node.edges_dynamic.iter().flatten() {
            for (edge_name, edge) in edges {
                for (branch, targets) in &edge.branches {
                    let label = format!("{type_key}:{edge_name}:{branch}");
                    for target in targets {
                        write_edge(out, &mut edge_id, source, target, Some(&label), node_ids);
                    }
                }
            }
        }
    }
    out.push_str("</edges>\n");
}

fn write_edge(
    out: &mut String,
    edge_id: &mut usize,
    source: usize,
    target_name: &str,
    label: Option<&str>,
    node_ids: &BTreeMap<&str, usize>,
) {
    let Some(&target) = node_ids.get(target_name) else {
        return;
    };
    match label {
        Some(label) => {
            let _ = write!(
                out,
                "<edge id=\"{}\" source=\"{}\" target=\"{}\" label=\"{}\"/>\n",
                edge_id,
                source,
                target,
                xml_escape(label)
            );
        }
        None => {
            let _ = write!(
                out,
                "<edge id=\"{}\" source=\"{}\" target=\"{}\"/>\n",
                edge_id, source, target
            );
        }
    }
    *edge_id += 1;
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use k9::snapshot;

    use super::*;
    use crate::GraphNode;
    use crate::types::map_graph::DynamicEdge;

    /// A `MapGraph` with a metric, a label, a property, and directed/tagged/
    /// dynamic edges — plus `&`, `<`, `>`, `"` in values to exercise escaping.
    #[test]
    fn test_map_graph_to_gexf() {
        let node_a = GraphNode {
            properties: None,
            labels: None,
            metrics: Some(BTreeMap::from([("size".to_string(), 1.5)])),
            edges_directed: Some(BTreeSet::from(["B".to_string()])),
            edges_tagged: Some(BTreeMap::from([(
                "lazy & async".to_string(),
                BTreeSet::from(["B".to_string()]),
            )])),
            edges_dynamic: Some(BTreeMap::from([(
                "ddd".to_string(),
                BTreeMap::from([(
                    "edge1".to_string(),
                    DynamicEdge {
                        branches: BTreeMap::from([(
                            "main".to_string(),
                            BTreeSet::from(["B".to_string()]),
                        )]),
                        metadata: None,
                    },
                )]),
            )])),
        };
        let node_b = GraphNode {
            properties: Some(BTreeMap::from([(
                "note".to_string(),
                "1 < 2 & \"ok\"".to_string(),
            )])),
            labels: Some(BTreeMap::from([(
                "tag<x>".to_string(),
                BTreeSet::from(["v1".to_string(), "v2".to_string()]),
            )])),
            metrics: None,
            edges_directed: None,
            edges_tagged: None,
            edges_dynamic: None,
        };
        let graph = MapGraph {
            nodes: BTreeMap::from([("A".to_string(), node_a), ("B".to_string(), node_b)]),
            traversal_config: None,
            graph_settings: None,
            entry_points: None,
            properties: BTreeMap::new(),
        };

        snapshot!(
            map_graph_to_gexf(&graph),
            r#"
<?xml version="1.0" encoding="UTF-8"?>
<gexf xmlns="http://gexf.net/1.3" version="1.3">
<meta>
<creator>unigraph</creator>
</meta>
<graph mode="static" defaultedgetype="directed">
<attributes class="node">
<attribute id="0" title="size" type="float"/>
<attribute id="1" title="tag&lt;x&gt;" type="string"/>
<attribute id="2" title="note" type="string"/>
</attributes>
<nodes>
<node id="0" label="A">
<attvalues>
<attvalue for="0" value="1.5"/>
</attvalues>
</node>
<node id="1" label="B">
<attvalues>
<attvalue for="1" value="v1,v2"/>
<attvalue for="2" value="1 &lt; 2 &amp; &quot;ok&quot;"/>
</attvalues>
</node>
</nodes>
<edges>
<edge id="0" source="0" target="1"/>
<edge id="1" source="0" target="1" label="lazy &amp; async"/>
<edge id="2" source="0" target="1" label="ddd:edge1:main"/>
</edges>
</graph>
</gexf>

"#
        );
    }
}
