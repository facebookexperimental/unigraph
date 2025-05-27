import { defineConfig } from "rolldown";

export default defineConfig([
  {
    input: "u-fe/index.tsx",
    output: {
      file: ".build/index.js",
    },
  },
  {
    input: "u-fe/Explorer.tsx",
    external: ["react", "react-dom", "react/jsx-runtime"],
    define: { "process.env.NODE_ENV": "'production'" },
    jsx: "react-jsx",
    output: {
      banner: `/**
 * @oncall unigraph
 * @nolint
 * @providesModule unigraph-explorer
 * @preserve-whitespace
 */`,
      file: ".build/unigraph-explorer-umd-build.js",
      format: "umd",
      name: "unigraph-explorer",
      minify: false,
    },
  },
]);
