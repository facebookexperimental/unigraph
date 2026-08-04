// Copyright (c) Meta Platforms, Inc. and affiliates.

//! When is a frame's content final?
//!
//! The `www` pipeline registers frames in order and fills them out of order
//! (see `build_and_store_www_budget`'s module docs). A threshold verdict is
//! only trustworthy if nothing can still appear between the sample being
//! judged and the sample it is judged against — so before omitting anything we
//! have to know whether a frame can still change.
//!
//! "Settled" means *this frame's content will not change again*, which is not
//! the same as "we ingested it":
//!
//! ```text
//! Empty      -> settled only once it has aged past the settle window; until
//!               then a build worker may still fill it
//! Error      -> settled; the graph pipeline never retries an Error frame
//! Full/Delta -> settled once history has a terminal verdict for it
//!               (Processed / Omitted / Error past the attempt cap)
//! ```
//!
//! The settle window is a bet, not a proof: a frame filled after it ages out
//! reintroduces the hazard for the frames after it. Size it from the observed
//! worst-case lag of the source pipeline, not from the ingest job's cadence.

use unigraph_storage_core::FrameType;
use unigraph_storage_core::HistoryStatusRow;
use unigraph_storage_core::Timestamp;

use crate::graph_history::MAX_ATTEMPTS;
use crate::graph_history::status::HistoryStatus;

/// Can `frame`'s content still change?
///
/// `settle_cutoff` is the timestamp before which an unfilled frame is presumed
/// abandoned.
pub fn is_frame_settled(
    frame_type: &FrameType,
    frame_timestamp: Timestamp,
    status: Option<&HistoryStatusRow>,
    settle_cutoff: Timestamp,
) -> bool {
    match frame_type {
        FrameType::Empty => frame_timestamp < settle_cutoff,
        FrameType::Error => true,
        FrameType::Full | FrameType::Delta => has_terminal_verdict(status),
    }
}

/// Has history already reached a verdict it will never revisit for this frame?
fn has_terminal_verdict(status: Option<&HistoryStatusRow>) -> bool {
    let Some(status) = status else {
        return false;
    };
    match status.status.parse::<HistoryStatus>() {
        Ok(HistoryStatus::Processed | HistoryStatus::Omitted) => true,
        Ok(HistoryStatus::Error) => status.attempts >= i64::from(MAX_ATTEMPTS),
        // A frame stamped Empty that is now Full/Delta was mirrored before it
        // was built — it still needs ingesting, so it is not settled.
        Ok(HistoryStatus::Empty) | Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use unigraph_storage_core::GraphID;

    use super::*;

    fn status(status: HistoryStatus, attempts: i64) -> HistoryStatusRow {
        HistoryStatusRow {
            graph_id: GraphID(1),
            status: status.to_string(),
            attempts,
            error_blob_key: None,
            omission_deferred: false,
        }
    }

    fn ts(secs: i64) -> Timestamp {
        Timestamp::from_unix_timestamp(secs)
    }

    #[test]
    fn settledness_by_frame_type_and_status() {
        let cutoff = ts(1000);
        let cases = [
            // (frame_type, frame ts, status, expected, why)
            (
                FrameType::Empty,
                ts(500),
                None,
                true,
                "an Empty frame older than the window is presumed abandoned",
            ),
            (
                FrameType::Empty,
                ts(1500),
                None,
                false,
                "a recent Empty frame may still be filled by a build worker",
            ),
            (
                FrameType::Error,
                ts(1500),
                None,
                true,
                "the graph pipeline never retries an Error frame",
            ),
            (
                FrameType::Full,
                ts(1500),
                None,
                false,
                "a built frame we have not ingested yet is still pending",
            ),
            (
                FrameType::Full,
                ts(1500),
                Some(status(HistoryStatus::Processed, 0)),
                true,
                "Processed is terminal",
            ),
            (
                FrameType::Delta,
                ts(1500),
                Some(status(HistoryStatus::Omitted, 0)),
                true,
                "Omitted is terminal",
            ),
            (
                FrameType::Full,
                ts(1500),
                Some(status(HistoryStatus::Error, 1)),
                false,
                "a retryable Error will be re-ingested, so it is not settled",
            ),
            (
                FrameType::Full,
                ts(1500),
                Some(status(HistoryStatus::Error, i64::from(MAX_ATTEMPTS))),
                true,
                "an Error past the attempt cap is abandoned, so it is terminal",
            ),
            (
                FrameType::Full,
                ts(1500),
                Some(status(HistoryStatus::Empty, 0)),
                false,
                "stamped Empty but now built — still needs ingesting",
            ),
        ];

        for (frame_type, frame_ts, row, expected, why) in cases {
            assert_eq!(
                is_frame_settled(&frame_type, frame_ts, row.as_ref(), cutoff),
                expected,
                "{why}"
            );
        }
    }
}
