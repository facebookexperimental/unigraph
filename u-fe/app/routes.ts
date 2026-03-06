// Copyright (c) Meta Platforms, Inc. and affiliates.

import { type RouteConfig, index, route } from "@react-router/dev/routes";

export default [
  index("routes/home.tsx"),
  route("timelines/:timelineId", "routes/timeline.tsx"),
  route("explorer", "routes/explorer.tsx"),
  route("explorer/:timelineId/:graphId", "routes/explorer-graph.tsx"),
] satisfies RouteConfig;
