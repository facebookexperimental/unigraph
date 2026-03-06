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

    tvc.with_tier_config();
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
F
  Tier: T1
G
  Tier: T1
H
  Tier: T1
I
  Tier: T1
J
  Tier: T3
K
  Tier: T2
L
  Tier: T1
M
  Tier: T1
N
  Tier: T1
O
  Tier: T1
P
  Tier: T1

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
D -> E
   tag: RDFD
E -> K
F -> G
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
F -> H
   branch: b1
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
F -> I
   branch: b2
   properties: {"type_key": "ddd", "edge_name": "ddd_1"}
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

    tvc.with_max_tier_idx(0 /* idx 0 = TIER 1 */);
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
E [UNREACHABLE]
F
  Tier: T1
G
  Tier: T1
H
  Tier: T1
I
  Tier: T1
J [UNREACHABLE]
K [UNREACHABLE]
L
  Tier: T1
M
  Tier: T1
N
  Tier: T1
O
  Tier: T1
P
  Tier: T1

"
    );
    Ok(())
}
