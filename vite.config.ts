// Copyright (c) Meta Platforms, Inc. and affiliates.

import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss(), reactRouter()],
  test: {
    exclude: ["e2e/**", "node_modules/**"],
  },
  server: {
    strictPort: true,
    hmr: {
      // Connect HMR WebSocket directly to Vite dev server,
      // not through the Rust reverse proxy on port 3000.
      // Without this, the browser hits an infinite reload loop.
      clientPort: 5173,
    },
  },
});
