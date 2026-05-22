// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Unified graph handle — identifies a graph by timeline, snapshot key, or
//! saved query config key.

use std::fmt;
use std::str::FromStr;

use crate::config_key::ConfigKeyLike;
use crate::config_key::GraphQueryConfigKey;
use crate::identifiers::GraphKey;
use crate::identifiers::GraphKeyOrTimelineID;
use crate::identifiers::TimelineID;

/// A parsed graph handle — three ways to reference a graph.
///
/// Handles come in three forms:
/// - `gqc_{hash}` — GQC key (content-addressed config with embedded graph ref)
/// - `{timeline}~{id}` — GraphKey (specific snapshot)
/// - `{timeline}` — TimelineID (latest graph)
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    typegen::TypeGen,
    PartialEq,
    Eq,
    Hash,
    unigraph_delta::Deltable
)]
#[serde(try_from = "String", into = "String")]
#[typegen(TypeScript("string"), Hack("string"))]
#[deltable(replace)]
pub enum GraphHandle {
    GqcKey(#[typegen(as = "String")] GraphQueryConfigKey),
    GraphKey(#[typegen(as = "String")] GraphKey),
    TimelineID(#[typegen(as = "String")] TimelineID),
}

impl fmt::Display for GraphHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphHandle::GqcKey(key) => write!(f, "{key}"),
            GraphHandle::GraphKey(key) => write!(f, "{key}"),
            GraphHandle::TimelineID(tid) => write!(f, "{tid}"),
        }
    }
}

impl From<GraphHandle> for String {
    fn from(handle: GraphHandle) -> Self {
        handle.to_string()
    }
}

impl TryFrom<String> for GraphHandle {
    type Error = anyhow::Error;

    fn try_from(s: String) -> anyhow::Result<Self> {
        s.parse()
    }
}

impl FromStr for GraphHandle {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        if s.starts_with(GraphQueryConfigKey::PREFIX) {
            return Ok(GraphHandle::GqcKey(s.parse()?));
        }

        match s.parse::<GraphKeyOrTimelineID>()? {
            GraphKeyOrTimelineID::GraphKey(key) => Ok(GraphHandle::GraphKey(key)),
            GraphKeyOrTimelineID::TimelineID(tid) => Ok(GraphHandle::TimelineID(tid)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_timeline_id() {
        let h: GraphHandle = "cargo".parse().unwrap();
        assert!(matches!(h, GraphHandle::TimelineID(_)));
        assert_eq!(h.to_string(), "cargo");
    }

    #[test]
    fn parse_graph_key() {
        let h: GraphHandle = "cargo~356".parse().unwrap();
        assert!(matches!(h, GraphHandle::GraphKey(_)));
        assert_eq!(h.to_string(), "cargo~356");
    }

    #[test]
    fn parse_gqc_key() {
        let h: GraphHandle = "gqc_1a2b3c4d5e6f7890".parse().unwrap();
        assert!(matches!(h, GraphHandle::GqcKey(_)));
        assert_eq!(h.to_string(), "gqc_1a2b3c4d5e6f7890");
    }

    #[test]
    fn roundtrip_serde() {
        let h: GraphHandle = "cargo~42".parse().unwrap();
        let json = serde_json::to_string(&h).unwrap();
        let parsed: GraphHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(h, parsed);
    }
}
