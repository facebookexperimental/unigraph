/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Namespace for external ID mappings.
 * 
 * Multiple timelines can share a namespace when they derive from the same
 * source (e.g. same git repo, different graph builders). A namespace like
 * `"my-repo/git"` groups all mappings for one source system.
 */
export type ExternalIDNamespace = string;