/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<410287c8bf9eeac5e4008a56b9e4855a>>
 */


/**
 * Namespace for external ID mappings.
 * 
 * Multiple timelines can share a namespace when they derive from the same
 * source (e.g. same git repo, different graph builders). A namespace like
 * `"my-repo/git"` groups all mappings for one source system.
 */
export type ExternalIDNamespace = string;