// Copyright (c) Meta Platforms, Inc. and affiliates.

import { type RouteConfig, index, route } from "@react-router/dev/routes";

// Graph handles live at the top level: `/:handle` for a single graph,
// `/:left/:right` for a delta view. Static routes above them win on match
// ranking, so `/timelines/foo` is never mistaken for a handle pair.
export default [
  index("routes/home.tsx"),
  route("timelines/:timelineId", "routes/timeline.tsx"),
  route(":handleL/:handleR", "routes/explorer.tsx", {
    id: "explorer-compare",
  }),
  route(":handleR", "routes/explorer.tsx", { id: "explorer-single" }),
] satisfies RouteConfig;
