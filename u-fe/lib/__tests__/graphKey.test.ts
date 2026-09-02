// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "vitest";
import { parseGraphKey } from "../graphKey";

test("parses a plain key", () => {
  expect(parseGraphKey("www-budget~223")).toEqual({
    timeline_id: "www-budget",
    graph_id: 223,
  });
});

test("splits on the last separator so timelines may contain '~'", () => {
  expect(parseGraphKey("a~b~7")).toEqual({ timeline_id: "a~b", graph_id: 7 });
});

test("rejects malformed keys", () => {
  expect(() => parseGraphKey("www-budget")).toThrow();
  expect(() => parseGraphKey("~7")).toThrow();
  expect(() => parseGraphKey("www-budget~")).toThrow();
  expect(() => parseGraphKey("www-budget~latest")).toThrow();
});
