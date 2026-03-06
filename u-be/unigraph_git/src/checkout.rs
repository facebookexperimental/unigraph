// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use git2::Repository;

/// Check out a specific commit by hash, detaching HEAD.
///
/// WARNING: This modifies the working tree of the repository.
pub fn checkout_commit(repo_path: &Path, hash: &str) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open git repository")?;
    let oid = git2::Oid::from_str(hash).with_context(|| format!("Invalid commit hash: {hash}"))?;
    let commit = repo
        .find_commit(oid)
        .with_context(|| format!("Commit not found: {hash}"))?;
    let tree = commit.tree().context("Failed to get commit tree")?;

    repo.checkout_tree(
        tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().force()),
    )
    .context("Failed to checkout tree")?;

    repo.set_head_detached(oid)
        .context("Failed to detach HEAD")?;

    Ok(())
}
