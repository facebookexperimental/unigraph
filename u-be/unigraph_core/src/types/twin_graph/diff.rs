// Copyright (c) Meta Platforms, Inc. and affiliates.

use crate::GraphSide;

bitflags::bitflags! {
    /// Value that represents the things that changed about a node
    /// between the left and right graphs of a TwinGraph.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct NodeDiff: u32 {
        const DOES_NOT_EXIST_IN_L = 0b_0001;
        const DOES_NOT_EXIST_IN_R = 0b_0010;
        const EDGES_CHANGED =       0b_0100;
        const METRICS_CHANGED =     0b_1000;
    }
}

impl NodeDiff {
    #[inline(always)]
    pub fn does_not_exist_in(self, side: GraphSide) -> bool {
        match side {
            GraphSide::Left => self.contains(NodeDiff::DOES_NOT_EXIST_IN_L),
            GraphSide::Right => self.contains(NodeDiff::DOES_NOT_EXIST_IN_R),
        }
    }

    #[inline(always)]
    pub fn mark_not_in_left(&mut self) {
        self.insert(NodeDiff::DOES_NOT_EXIST_IN_L);
    }

    #[inline(always)]
    pub fn mark_not_in_right(&mut self) {
        self.insert(NodeDiff::DOES_NOT_EXIST_IN_R);
    }

    pub fn debug(&self) -> String {
        let mut result: Vec<&str> = vec![];

        match (
            self.does_not_exist_in(GraphSide::Left),
            self.does_not_exist_in(GraphSide::Right),
        ) {
            (true, true) | (false, false) => {}
            (false, true) => result.push("REMOVED"),
            (true, false) => result.push("ADDED"),
        }

        if self.contains(NodeDiff::EDGES_CHANGED) {
            result.push("EDGES_CHANGED");
        }
        if self.contains(NodeDiff::METRICS_CHANGED) {
            result.push("METRICS_CHANGED");
        }

        result.join(" | ")
    }
}

impl serde::Serialize for NodeDiff {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.bits())
    }
}
