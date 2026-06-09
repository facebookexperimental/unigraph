// Copyright (c) Meta Platforms, Inc. and affiliates.

import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
	testDir: "./e2e",
	timeout: 60_000,
	retries: 0,
	use: {
		baseURL: "http://localhost:3000",
	},
	projects: [
		{
			name: "chromium",
			use: { ...devices["Desktop Chrome"] },
		},
	],
	webServer: {
		command:
			"python3 tasks/task.py build wasm && npx react-router build && cargo run -p unigraph -- serve --release -f e2e/fixtures/explore_graph.json",
		port: 3000,
		reuseExistingServer: true,
		timeout: 120_000,
		stdout: "pipe",
		stderr: "pipe",
	},
});
