// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Result;
use k9::snapshot;

use crate::TraversalConfig;
use crate::tests::test_graphs::make_test_array_graph_2;
use crate::tests::test_utils::array_graph_test_trait::ArrayGraphTestTrait;
use crate::tests::test_utils::print_arrows;
use crate::tests::test_utils::traversal_config_test_trait::TraversalConfigTestTrait;

#[test]
fn test_force_children_of() -> Result<()> {
    let mut ag = make_test_array_graph_2()?;
    let mut tvc = TraversalConfig::default();

    tvc.set_force_children_of("O", false).with_tier_config();
    ag.apply_traversal_config(tvc.clone())?;

    snapshot!(
        ag.print_nodes(),
        "
A
  Tier: T1
B
  Tier: T1
C
  Tier: T4
D
  Tier: T1
E
  Tier: T2
F [UNREACHABLE]
G [UNREACHABLE]
H [UNREACHABLE]
I [UNREACHABLE]
J
  Tier: T3
K
  Tier: T2
L
  Tier: T1
M
  Tier: T1
N [UNREACHABLE]
O
  Tier: T1
P [UNREACHABLE]

"
    );

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
F -> H
   branch: b1
   properties: {"type": "DDD"}
F -> I
   branch: b2
   properties: {"type": "DDD"}
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

    tvc.with_max_tier_idx(1 /* idx 1 = TIER 2 */);
    ag.apply_traversal_config(tvc.clone())?;
    snapshot!(
        ag.print_nodes(),
        "
A
  Tier: T1
B
  Tier: T1
C [UNREACHABLE]
D
  Tier: T1
E
  Tier: T2
F [UNREACHABLE]
G [UNREACHABLE]
H [UNREACHABLE]
I [UNREACHABLE]
J [UNREACHABLE]
K
  Tier: T2
L
  Tier: T1
M
  Tier: T1
N [UNREACHABLE]
O
  Tier: T1
P [UNREACHABLE]

"
    );
    Ok(())
}
