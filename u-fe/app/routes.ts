// Copyright (c) Meta Platforms, Inc. and affiliates.

import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [
  index("routes/home.tsx"),
  route("timelines/:timelineId", "routes/timeline.tsx"),
  route("explorer/local", "routes/explorer.tsx", { id: "explorer-local" }),
  route("explorer/:handleR/:handleL", "routes/explorer.tsx", {
    id: "explorer-compare",
  }),
  route("explorer/:handleR", "routes/explorer.tsx", {
    id: "explorer-handle",
  }),
] satisfies RouteConfig;
