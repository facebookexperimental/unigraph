// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::NodeIDX;
use crate::types::array_graph::{self};

pub type MessageID = String;

/// Traversal config messages are little pieces of extra information
/// that we can show in the UI to help users understand why a certain
/// edge was followed or not followed.
/// e.g. (this edge was not followed because it was explicitly excluded because it
/// contained a certain tag)
/// The messages involve strings, and since there are potentially millions of
/// edges in the graph we can't just associate every edge with a message.
/// Instead, we use a message ID to refer to a message and when UI wants to
/// render a specific edge with a message we can lazily compile that message
/// and show it to the user.
///
/// Messages are strings that support template literals, so we can define a
/// template and it will render the message with additional info about
/// the nodes and edges involved.
///
/// Template literals:
///     %points_from%   - name of the node the edge is coming from
///     %points_to%     - name of the node the edge is pointing to
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(export)]
pub struct Message(pub String);

const MESSAGE_TEMPLATE_POINTS_FROM: &str = "%points_from%";
const MESSAGE_TEMPLATE_POINTS_TO: &str = "%points_to%";

impl Message {
    pub fn render(&self, ag: &array_graph::ArrayGraph, points_from: NodeIDX) -> Result<String> {
        let mut result = self.0.clone();

        if result.contains(MESSAGE_TEMPLATE_POINTS_FROM) {
            let points_from_name = ag.node_names_ordered.idx_to_name(points_from);
            result = result.replace(MESSAGE_TEMPLATE_POINTS_FROM, points_from_name);
        }

        if result.contains(MESSAGE_TEMPLATE_POINTS_TO) {
            let points_to_name = ag.node_names_ordered.idx_to_name(points_from);
            result = result.replace(MESSAGE_TEMPLATE_POINTS_TO, points_to_name);
        }
        Ok(result)
    }
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        Message(s.to_string())
    }
}

/// These messages are indexed in a Vec and contain the `ID -> IDX` mapping.
/// When traversal config is applied to the graph, EdgeFlags can refer to these
/// messages by their numeric index. (obviously we can't refer to them by string
/// IDs cause there are potentially millions of edges)
#[derive(Default)]
pub struct IndexedMessages {
    pub message_id_to_idx: BTreeMap<MessageID, u8>,
    pub messages: Vec<Message>,
}

impl IndexedMessages {
    pub fn new_with_builtin(messages: &BTreeMap<MessageID, Message>) -> Self {
        let mut with_builtin = BuiltInMessages::get_all();

        with_builtin.extend(
            messages
                .iter()
                .map(|(id, msg)| (id.to_string(), msg.clone())),
        );

        let mut message_map = BTreeMap::new();
        let mut messages = Vec::new();
        for (idx, (id, message)) in with_builtin.into_iter().enumerate() {
            message_map.insert(id, idx as u8);
            messages.push(message);
        }
        IndexedMessages {
            message_id_to_idx: message_map,
            messages,
        }
    }

    pub fn get(&self, id: &str) -> Option<u8> {
        self.message_id_to_idx.get(id).copied()
    }

    pub fn get_or_default(&self, id: &Option<MessageID>, default: &str) -> Option<u8> {
        match id {
            Some(id) => self.get(id).or_else(|| self.get(default)),
            None => self.get(default),
        }
    }

    pub fn get_by_idx(&self, idx: u8) -> Option<&Message> {
        self.messages.get(idx as usize)
    }
}

pub struct BuiltInMessages;
impl BuiltInMessages {
    pub const NODE_FORCE_EXCLUDED_ID: &str = "node_force_excluded";

    const NODE_FORCE_EXCLUDED_MESSAGE: &str = "This edge was excluded because the node `%points_to%` was force excluded from the traversal using `force_nodes` config.";

    const ALL_MESSAGES: &'static [(&'static str, &'static str)] = &[(
        Self::NODE_FORCE_EXCLUDED_ID,
        Self::NODE_FORCE_EXCLUDED_MESSAGE,
    )];

    fn get_all() -> BTreeMap<String, Message> {
        Self::ALL_MESSAGES
            .iter()
            .map(|(id, message)| (id.to_string(), Message(message.to_string())))
            .collect::<BTreeMap<_, _>>()
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;
    use maplit::btreemap;

    use super::*;
    use crate::Decision;
    use crate::TraversalConfig;
    use crate::tests::test_graphs::make_test_array_graph_2;
    use crate::tests::test_utils::print_arrows;

    #[test]
    fn test_render_message() -> Result<()> {
        let mut g = make_test_array_graph_2()?;

        g.apply_traversal_config(TraversalConfig {
            force_nodes: btreemap! { "I".into() => Decision { include: false, message_id: None } },
            ..Default::default()
        })?;
        snapshot!(
            print_arrows(&g),
            r#"
A -> B
A -> D
B -> C
   tag: BL
B -> J
   tag: RD
D -> F
D -> E
   tag: RDFD
E -> K
F -> G
   branch: b1
   properties: {"type": "DDD"}
F -> H
   branch: b1
   properties: {"type": "DDD"}
F -> I
   branch: b2
   properties: {"type": "DDD"}
   message: This edge was excluded because the node `F` was force excluded from the traversal using `force_nodes` config.
J -> K
L -> D
L -> M
M -> O
N -> M
O -> N
O -> P
O -> F
   tag: BL
"#
        );

        Ok(())
    }
}
