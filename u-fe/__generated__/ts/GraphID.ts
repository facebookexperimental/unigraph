/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<40912d618d9287f46078e8abe460caee>>
 */


/**
 * Unique identifier for a graph within a timeline.
 * 
 * Sequential integer assigned during ingestion. Sorts naturally for
 * correct frame ordering when multiple frames share the same timestamp.
 */
export type GraphID = number;