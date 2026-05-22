// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use git2::Repository;

use crate::commit::CommitInfo;

/// Find the default branch ref (`refs/heads/main` or `refs/heads/master`).
fn find_default_branch(repo: &Repository) -> Result<git2::Reference<'_>> {
    for name in &["refs/heads/main", "refs/heads/master"] {
        if let Ok(r) = repo.find_reference(name) {
            return Ok(r);
        }
    }
    Err(anyhow!("Could not find 'main' or 'master' branch"))
}

/// Resolve the starting commit for history traversal.
///
/// If HEAD points to a branch, uses HEAD. If HEAD is detached,
/// falls back to `main` or `master`.
fn resolve_start_commit(repo: &Repository) -> Result<git2::Commit<'_>> {
    let head = repo.head().context("Failed to read HEAD")?;
    if head.is_branch() {
        head.peel_to_commit()
            .context("HEAD does not point to a commit")
    } else {
        let branch = find_default_branch(repo)?;
        branch
            .peel_to_commit()
            .context("Default branch does not point to a commit")
    }
}

/// Walk first-parent history back to the initial commit.
///
/// Starts from the current branch, or falls back to `main`/`master`
/// if HEAD is detached. Returns commits in chronological order (oldest first).
/// Merge commits are included as single units; branch-only
/// commits (reachable only via non-first-parent paths) are excluded.
pub fn collect_linear_history(repo_path: &Path) -> Result<Vec<CommitInfo>> {
    collect_linear_history_since(repo_path, None)
}

/// Walk first-parent history from HEAD back to `since_commit` (exclusive).
///
/// Returns only commits newer than `since_commit`, in chronological order
/// (oldest first). If `since_commit` is `None`, returns the full history.
pub fn collect_linear_history_since(
    repo_path: &Path,
    since_commit: Option<&str>,
) -> Result<Vec<CommitInfo>> {
    let repo = Repository::open(repo_path).context("Failed to open git repository")?;
    let start_commit = resolve_start_commit(&repo)?;

    let mut revwalk = repo.revwalk().context("Failed to create revwalk")?;
    revwalk.push(start_commit.id())?;
    revwalk.simplify_first_parent()?;

    // Hide the known commit and all its ancestors — walk stops there
    if let Some(hash) = since_commit {
        let oid =
            git2::Oid::from_str(hash).with_context(|| format!("Invalid commit hash: {hash}"))?;
        revwalk.hide(oid)?;
    }

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result.context("Failed to iterate revwalk")?;
        let commit = repo
            .find_commit(oid)
            .with_context(|| format!("Failed to find commit {oid}"))?;

        let time = commit.time();
        let timestamp = unigraph_timestamp::Timestamp::from_unix_timestamp(time.seconds());

        commits.push(CommitInfo {
            hash: oid.to_string(),
            timestamp,
            summary: commit.summary().unwrap_or("").to_string(),
        });
    }

    // revwalk returns newest-first; reverse to get oldest-first
    commits.reverse();
    Ok(commits)
}
