// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::cmp::Ordering;

use crate::ArrayGraphNodes;
use crate::NodeIDX;
use crate::remap_utils::RemapContext;

/// Remap tables for a TwinGraph. Maps between merged indices and each side's local indices.
/// No string copies — just integer arithmetic from a sorted merge-walk.
pub struct TwinRemap {
    pub merged_len: usize,
    pub twin_to_l: Vec<Option<NodeIDX>>,
    pub twin_to_r: Vec<Option<NodeIDX>>,
    pub l_to_twin: Vec<NodeIDX>,
    pub r_to_twin: Vec<NodeIDX>,
}

impl TwinRemap {
    /// Build remap tables by merge-walking two sorted name lists.
    /// No strings are copied — we only compare names and record index relationships.
    pub fn build(l: &ArrayGraphNodes, r: &ArrayGraphNodes) -> Self {
        let mut twin_to_l = Vec::new();
        let mut twin_to_r = Vec::new();
        let mut l_to_twin = vec![NodeIDX(0); l.len()];
        let mut r_to_twin = vec![NodeIDX(0); r.len()];

        let mut l_iter = l.node_idx_iter();
        let mut r_iter = r.node_idx_iter();

        let mut next_l = l_iter.next();
        let mut next_r = r_iter.next();
        let mut merged_idx = 0u32;

        loop {
            match (next_l, next_r) {
                (Some(l_idx), Some(r_idx)) => {
                    let l_name = l.idx_to_name(l_idx);
                    let r_name = r.idx_to_name(r_idx);

                    match l_name.cmp(r_name) {
                        Ordering::Equal => {
                            twin_to_l.push(Some(l_idx));
                            twin_to_r.push(Some(r_idx));
                            l_to_twin[usize::from(l_idx)] = NodeIDX(merged_idx);
                            r_to_twin[usize::from(r_idx)] = NodeIDX(merged_idx);
                            next_l = l_iter.next();
                            next_r = r_iter.next();
                        }
                        Ordering::Less => {
                            twin_to_l.push(Some(l_idx));
                            twin_to_r.push(None);
                            l_to_twin[usize::from(l_idx)] = NodeIDX(merged_idx);
                            next_l = l_iter.next();
                        }
                        Ordering::Greater => {
                            twin_to_l.push(None);
                            twin_to_r.push(Some(r_idx));
                            r_to_twin[usize::from(r_idx)] = NodeIDX(merged_idx);
                            next_r = r_iter.next();
                        }
                    }
                }
                (Some(l_idx), None) => {
                    twin_to_l.push(Some(l_idx));
                    twin_to_r.push(None);
                    l_to_twin[usize::from(l_idx)] = NodeIDX(merged_idx);
                    next_l = l_iter.next();
                }
                (None, Some(r_idx)) => {
                    twin_to_l.push(None);
                    twin_to_r.push(Some(r_idx));
                    r_to_twin[usize::from(r_idx)] = NodeIDX(merged_idx);
                    next_r = r_iter.next();
                }
                (None, None) => break,
            }
            merged_idx += 1;
        }

        TwinRemap {
            merged_len: merged_idx as usize,
            twin_to_l,
            twin_to_r,
            l_to_twin,
            r_to_twin,
        }
    }

    /// Resolve a merged index to a name by looking it up in whichever side has it.
    /// Prefers R (the "current" graph).
    pub fn merged_idx_to_name<'a>(
        &self,
        l: &'a ArrayGraphNodes,
        r: &'a ArrayGraphNodes,
        merged_idx: NodeIDX,
    ) -> &'a str {
        if let Some(r_idx) = self.twin_to_r[merged_idx] {
            r.idx_to_name(r_idx)
        } else {
            l.idx_to_name(
                self.twin_to_l[merged_idx].expect(
                    "merged_idx_to_name: every merged index must exist on at least one side",
                ),
            )
        }
    }
}

/// Merges two sorted `ArrayGraphNodes` into one, deduplicating shared names.
/// Returns the merged nodes plus `RemapContext` for each side (old idx → new idx).
/// Used by delta derive to compute diffs in a unified namespace.
pub fn merge_node_names(
    a: &ArrayGraphNodes,
    b: &ArrayGraphNodes,
) -> (ArrayGraphNodes, RemapContext, RemapContext) {
    let mut ctx_a = RemapContext::default();
    let mut ctx_b = RemapContext::default();

    let mut names = String::with_capacity(a.as_parts().0.len().max(b.as_parts().0.len()));
    let mut offsets = Vec::with_capacity(a.len().max(b.len()) + 1);
    offsets.push(0);

    let mut a_iter = a.node_idx_iter();
    let mut b_iter = b.node_idx_iter();

    let mut next_a = a_iter.next();
    let mut next_b = b_iter.next();
    let mut current_idx = NodeIDX::from(0u32);

    loop {
        match (next_a, next_b) {
            (Some(a_idx), Some(b_idx)) => {
                let a_str = a.idx_to_name(a_idx);
                let b_str = b.idx_to_name(b_idx);

                match a_str.cmp(b_str) {
                    Ordering::Equal => {
                        names.push_str(a_str);
                        offsets.push(names.len());
                        next_a = a_iter.next();
                        next_b = b_iter.next();
                        ctx_a.original_positions.push(Some(a_idx));
                        ctx_b.original_positions.push(Some(b_idx));
                        ctx_a.mappings.push(Some(current_idx));
                        ctx_b.mappings.push(Some(current_idx));
                    }
                    Ordering::Less => {
                        names.push_str(a_str);
                        offsets.push(names.len());
                        next_a = a_iter.next();
                        ctx_a.original_positions.push(Some(a_idx));
                        ctx_b.original_positions.push(None);
                        ctx_a.mappings.push(Some(current_idx));
                    }
                    Ordering::Greater => {
                        names.push_str(b_str);
                        offsets.push(names.len());
                        next_b = b_iter.next();
                        ctx_b.original_positions.push(Some(b_idx));
                        ctx_a.original_positions.push(None);
                        ctx_b.mappings.push(Some(current_idx));
                    }
                }
            }
            (Some(a_idx), None) => {
                let a_str = a.idx_to_name(a_idx);
                names.push_str(a_str);
                offsets.push(names.len());
                next_a = a_iter.next();
                ctx_a.original_positions.push(Some(a_idx));
                ctx_b.original_positions.push(None);
                ctx_a.mappings.push(Some(current_idx));
            }
            (None, Some(b_idx)) => {
                let b_str = b.idx_to_name(b_idx);
                names.push_str(b_str);
                offsets.push(names.len());
                next_b = b_iter.next();
                ctx_b.original_positions.push(Some(b_idx));
                ctx_a.original_positions.push(None);
                ctx_b.mappings.push(Some(current_idx));
            }
            (None, None) => {
                return (ArrayGraphNodes::from_parts(names, offsets), ctx_a, ctx_b);
            }
        }
        current_idx.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use k9::snapshot;

    use super::*;

    fn make_nodes(names: &[&str]) -> ArrayGraphNodes {
        let mut s = String::new();
        let mut offsets = vec![0];
        for name in names {
            s.push_str(name);
            offsets.push(s.len());
        }
        ArrayGraphNodes::from_parts(s, offsets)
    }

    fn merge_two(a: &[&str], b: &[&str]) -> String {
        let a = make_nodes(a);
        let b = make_nodes(b);
        let (merged, ctx_a, ctx_b) = merge_node_names(&a, &b);
        let names = merged.node_names_iter().collect::<Vec<_>>().join(", ");

        let mut result = String::new();
        result.push_str(&names);
        result.push_str(&format!("\n\nctx a:\n{}\n", ctx_a.debug()));
        result.push_str(&format!("ctx b:\n{}", ctx_b.debug()));
        result
    }

    #[test]
    fn test_merge() -> anyhow::Result<()> {
        snapshot!(
            merge_two(&["a", "b"], &["c", "d"]),
            "
a, b, c, d

ctx a:
org: 0, 1, _, _
map: 0, 1

ctx b:
org: _, _, 0, 1
map: 2, 3

"
        );
        snapshot!(
            merge_two(&["a", "b"], &["b", "c"]),
            "
a, b, c

ctx a:
org: 0, 1, _
map: 0, 1

ctx b:
org: _, 0, 1
map: 1, 2

"
        );
        snapshot!(
            merge_two(&["a", "b"], &[]),
            "
a, b

ctx a:
org: 0, 1
map: 0, 1

ctx b:
org: _, _
map:

"
        );
        snapshot!(
            merge_two(&[], &["b", "c"]),
            "
b, c

ctx a:
org: _, _
map:

ctx b:
org: 0, 1
map: 0, 1

"
        );
        snapshot!(
            merge_two(&["c", "d"], &["a", "f"]),
            "
a, c, d, f

ctx a:
org: _, 0, 1, _
map: 1, 2

ctx b:
org: 0, _, _, 1
map: 0, 3

"
        );
        snapshot!(
            merge_two(&["a"], &["a"]),
            "
a

ctx a:
org: 0
map: 0

ctx b:
org: 0
map: 0

"
        );
        snapshot!(
            merge_two(&[], &[]),
            "


ctx a:
org:
map:

ctx b:
org:
map:

"
        );
        snapshot!(
            merge_two(&["a", "c", "e"], &["b", "d"]),
            "
a, b, c, d, e

ctx a:
org: 0, _, 1, _, 2
map: 0, 2, 4

ctx b:
org: _, 0, _, 1, _
map: 1, 3

"
        );
        Ok(())
    }
}
