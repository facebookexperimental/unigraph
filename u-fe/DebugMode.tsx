// Copyright (c) Meta Platforms, Inc. and affiliates.

/// Gate extra controls that are useful for debugging but not so much for normal use.
/// Enabled with `?debug=1` in the URL.
export const IS_DEBUG_MODE =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("debug") === "1";
