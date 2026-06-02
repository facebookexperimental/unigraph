// Copyright (c) Meta Platforms, Inc. and affiliates.

import { defineConfig, devices } from "@playwright/test";
import {getBaseUrl} from '../../libs/playwright/src';
import * as path from 'path';

const appDir = path.resolve(__dirname);

// getBaseUrl handles all environments:
// 1. PLAYWRIGHT_BASE_URL env var (deployed previews)
// 2. NEST_APP_SERVER env var (CI via Buck's nest_app_resource_provider)
// 3. Local dev server (hostname + port from .next/dev/status.json)
const baseURL = getBaseUrl(appDir);

export default defineConfig({
	testDir: "./e2e",
	timeout: 60_000,
	retries: 0,
	use: {
		baseURL,
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
