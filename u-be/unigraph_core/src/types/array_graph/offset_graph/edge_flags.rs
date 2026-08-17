// Copyright (c) Meta Platforms, Inc. and affiliates.

use anyhow::Context;
use anyhow::Result;

use crate::types::array_graph::tiers::ALL_TIER_FLAGS;
use crate::types::array_graph::tiers::TIER_FLAGS;
use crate::types::array_graph::tiers::flags_to_tier_idx;
use crate::types::array_graph::tiers::tier_idx_to_flags;

pub enum EdgeType {
    Directed,
    Tagged,
    Dynamic,
}

bitflags::bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct EdgeFlags: u32 {
        const IS_TAGGED =                   0b0000_0000_0000_0001;
        const IS_DYNAMIC =                  0b0000_0000_0000_0010;
        const EXCLUDED =                    0b0000_0000_0000_0100;

        const TRANSITIONS_TO_TIER_IDX_0 =   TIER_FLAGS[0];
        const TRANSITIONS_TO_TIER_IDX_1 =   TIER_FLAGS[1];
        const TRANSITIONS_TO_TIER_IDX_2 =   TIER_FLAGS[2];
        const TRANSITIONS_TO_TIER_IDX_3 =   TIER_FLAGS[3];
        const TRANSITIONS_TO_TIER_IDX_4 =   TIER_FLAGS[4];
        const TRANSITIONS_TO_TIER_IDX_5 =   TIER_FLAGS[5];
        const TRANSITIONS_TO_TIER_IDX_6 =   TIER_FLAGS[6];
        const TRANSITIONS_TO_TIER_IDX_7 =   TIER_FLAGS[7];
        const ALL_TIERS =                   ALL_TIER_FLAGS;

        /// These bits used to encode traversal message index
        /// that is associated with that edge.
        /// see [`unigraph_core::traversal::messages`] for more details.
        /// the default bits value of 0000_0000 is reserved for "no message".
        /// The rest of the bits are holding a shifted by 1 value of the message idx
        /// This message IDX is using 8 bits, so it can encode up to 255 messages.
        /// plus 1111_1111 value (256) is reserved for "no message".
        /// The rest of the values contain a shifted message idx.
        /// e.g.
        /// 0000_0001 is message idx 0,
        /// 0000_0010 is message idx 1,
        /// 0000_0011 is message idx 2,
        /// ...and so on.
        const ENCODED_MESSAGE_IDX_BITS =  0b1111_1111_0000_0000;
    }
}

impl EdgeFlags {
    /// All 32 bits, because the tier block lives at bits 16..24 — a narrower
    /// rendering would silently hide it.
    pub fn to_binary_string(self) -> String {
        let binary = format!("{:032b}", self.bits());
        let mut result = String::with_capacity(39); // 32 digits + 7 separators
        for (i, c) in binary.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                result.push('_');
            }
            result.push(c);
        }
        result
    }

    pub fn dbg(self) -> String {
        format!(
            "
flags: {}
excluded: {}
transitions to tier idx: {:?}
",
            self.to_binary_string(),
            self.is_excluded(),
            self.transitions_to_tier_idx()
        )
    }

    #[inline(always)]
    pub fn is_excluded(&self) -> bool {
        self.contains(EdgeFlags::EXCLUDED)
    }

    pub fn edge_type(self) -> EdgeType {
        if self.contains(EdgeFlags::IS_DYNAMIC) {
            EdgeType::Dynamic
        } else if self.contains(EdgeFlags::IS_TAGGED) {
            EdgeType::Tagged
        } else {
            EdgeType::Directed
        }
    }

    pub fn is_tagged_or_dynamic(self) -> bool {
        self.intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC)
    }

    pub fn exclude(&mut self) {
        self.insert(EdgeFlags::EXCLUDED);
    }

    pub fn exclude_with_message(&mut self, message_idx: Option<u8>) -> Result<()> {
        self.exclude();
        if let Some(message_idx) = message_idx {
            self.set_message_idx(message_idx)?;
        } else {
            self.remove_message_idx();
        }
        Ok(())
    }

    pub fn include_with_message(&mut self, message_idx: Option<u8>) -> Result<()> {
        self.include();
        if let Some(message_idx) = message_idx {
            self.set_message_idx(message_idx)?;
        } else {
            self.remove_message_idx();
        }
        Ok(())
    }

    pub fn include(&mut self) {
        self.remove(EdgeFlags::EXCLUDED);
    }

    pub fn transitions_to_tier_idx(self) -> Option<usize> {
        flags_to_tier_idx(self.intersection(EdgeFlags::ALL_TIERS).bits())
    }

    pub fn set_transitions_to_tier_idx(&mut self, tier_idx: usize) -> Result<()> {
        let tier_transition_flags = tier_idx_to_flags(tier_idx)?;
        let flags = EdgeFlags::from_bits(tier_transition_flags)
            .with_context(|| format!("Invalid tier flags: {tier_transition_flags:#b}"))?;
        self.insert(flags);
        Ok(())
    }

    pub fn get_message_idx(self) -> Option<u8> {
        let bits = (self & EdgeFlags::ENCODED_MESSAGE_IDX_BITS).bits() >> 8;
        if bits == 0 {
            None
        } else {
            Some((bits - 1) as u8)
        }
    }

    pub fn remove_message_idx(&mut self) {
        self.remove(EdgeFlags::ENCODED_MESSAGE_IDX_BITS);
    }

    pub fn set_message_idx(&mut self, message_idx: u8) -> Result<()> {
        if message_idx > 254 {
            anyhow::bail!(
                "Message index must be between 0 and 254. (255 is reserved for 'no message')
Value provided: {message_idx}."
            );
        }

        // clear the bits first
        self.remove_message_idx();

        let bits = (message_idx as u32 + 1) << 8;
        let flags = EdgeFlags::from_bits(bits)
            .with_context(|| format!("Invalid message flags: {bits:#b}"))?;
        self.insert(flags);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use k9::*;

    use super::*;
    use crate::NodeIDX;
    use crate::types::array_graph::offset_graph::Edge;

    #[test]
    fn edge_test() {
        assert_equal!(std::mem::size_of::<Edge>(), 8);
    }

    #[test]
    fn test_edge_flags() -> Result<()> {
        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::IS_TAGGED);
        assert_equal!(
            edge.flags.to_binary_string(),
            "0000_0000_0000_0000_0000_0000_0000_0001"
        );
        assert_equal!(edge.flags.contains(EdgeFlags::IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::IS_TAGGED), true);
        assert_equal!(edge.flags.intersects(EdgeFlags::IS_DYNAMIC), false);
        assert_equal!(
            edge.flags
                .intersects(EdgeFlags::IS_TAGGED | EdgeFlags::IS_DYNAMIC),
            true
        );

        assert_equal!(
            edge.flags.to_binary_string(),
            "0000_0000_0000_0000_0000_0000_0000_0001"
        );

        let edge = Edge::new_with_flags(NodeIDX(1), EdgeFlags::IS_DYNAMIC);
        assert_equal!(edge.flags.contains(EdgeFlags::IS_DYNAMIC), true);

        assert_equal!(
            edge.flags.to_binary_string(),
            "0000_0000_0000_0000_0000_0000_0000_0010"
        );

        Ok(())
    }

    #[test]
    fn test_edge_flags_message_idx() -> Result<()> {
        let mut f = EdgeFlags::default();

        assert_equal!(f.get_message_idx(), None);
        f.set_message_idx(1)?;
        assert_equal!(f.get_message_idx(), Some(1));

        f.set_message_idx(20)?;
        assert_equal!(f.get_message_idx(), Some(20));

        assert_err_matches_regex!(f.set_message_idx(255), "255 is reserved for 'no message");

        Ok(())
    }
}
