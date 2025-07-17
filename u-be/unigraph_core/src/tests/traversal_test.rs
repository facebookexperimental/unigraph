// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;

use crate::TraversalConfig;
use crate::tests::test_graphs::make_test_array_graph_2;
use crate::tests::test_utils::print_arrows;
use crate::tests::test_utils::traversal_config_utils::TraversalConfigTestUtils;

#[test]
fn test_force_children_of() -> Result<()> {
    let mut ag = make_test_array_graph_2()?;
    let mut tvc = TraversalConfig::default();

    tvc.set_force_children_of("O", false);
    ag.apply_traversal_config(tvc)?;

    snapshot!(
        print_arrows(&ag),
        r#"
A -> B
A -> D
B -> C
   tag: BL
B -> J
   tag: RD
D -> F
   message: This edge was EXCLUDED because the node `D` is a child of a node whose children were excluded from the traversal using `force_children_of` config.
D -> E
   tag: RDFD
E -> K
F -> G
   branch: b1
   properties: {"type": "DDD"}
   message: This edge was INCLUDED because it matched the `force_dynamic` config for the branch `b1` with properties `{"type": "DDD"}`.
F -> H
   branch: b1
   properties: {"type": "DDD"}
   message: This edge was INCLUDED because it matched the `force_dynamic` config for the branch `b1` with properties `{"type": "DDD"}`.
F -> I
   branch: b2
   properties: {"type": "DDD"}
   message: This edge was INCLUDED because it matched the `force_dynamic` config for the branch `b2` with properties `{"type": "DDD"}`.
J -> K
L -> D
L -> M
M -> O
N -> M
O -> N
   message: This edge was EXCLUDED because the node `O` is a child of a node whose children were excluded from the traversal using `force_children_of` config.
O -> P
   message: This edge was EXCLUDED because the node `O` is a child of a node whose children were excluded from the traversal using `force_children_of` config.
O -> F
   tag: BL
   message: This edge was EXCLUDED because the node `O` is a child of a node whose children were excluded from the traversal using `force_children_of` config.
"#
    );
    Ok(())
}
