/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated
 */


/**
 * Unique identifier for a graph within a timeline.
 * 
 * Sequential integer assigned during ingestion. Sorts naturally for
 * correct frame ordering when multiple frames share the same timestamp.
 */
export type GraphID = number;