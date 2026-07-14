// Copyright (c) Meta Platforms, Inc. and affiliates.

import { defineConfig } from "rolldown";
import { dts } from "rolldown-plugin-dts";

export default defineConfig({
  input: "u-fe/Unigraph.tsx",
  external: ["react", "react-dom", "react/jsx-runtime"],
  transform: {
    define: { "process.env.NODE_ENV": "'production'" },
    jsx: "react-jsx",
  },
  output: {
    dir: ".build/js",
    format: "esm",
    minify: false,
  },
  plugins: [docblock(), stripSignedSource(), dts()],
});

// SignedSource tokens from the bundled `__generated__/ts/*.ts` files no longer match
// the merged bundle, so strip them to keep signature-verification tooling off the
// build artifact. The bare `@generated` marker is left intact. Runs in
// `generateBundle` (after `dts()` has inlined the declaration content) over every
// emitted file, chunk or asset.
const SIGNED_SOURCE_TOKEN = / SignedSource<<[0-9a-fA-F]+>>/g;

function stripSignedSource() {
  return {
    name: "strip-signed-source",
    generateBundle(_options: unknown, bundle: Record<string, any>) {
      for (const file of Object.values(bundle)) {
        if (file.type === "chunk") {
          file.code = file.code.replace(SIGNED_SOURCE_TOKEN, "");
        } else if (typeof file.source === "string") {
          file.source = file.source.replace(SIGNED_SOURCE_TOKEN, "");
        }
      }
    },
  };
}

const DOCBLOCK = `/**
 * @oncall unigraph
 * @nolint
 * @preserve-whitespace
 * @${"generated"}
 */`;

function docblock() {
  return {
    name: "docblock",
    renderChunk(code: string) {
      return `${DOCBLOCK}\n\n${code}`;
    },
  };
}
