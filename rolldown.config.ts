import { readFileSync } from "node:fs";
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
      file: ".build/unigraph-explorer-intern.js",
      format: "umd",
      name: "unigraph-explorer-intern",
      minify: false,
    },
    plugins: [injectCSS()],
  },
]);

const DOCBLOCK = `/**
 * @oncall unigraph
 * @nolint
 * @providesModule unigraph-explorer-intern
 * @preserve-whitespace
 * @${"generated"}
 */`;
const CSS_CONTENT = readFileSync(`${__dirname}/.build/output.css`, "utf-8");

function injectCSS() {
  return {
    name: "inject-css",
    renderChunk(code: string) {
      const injectCss = `document.head.appendChild(document.createElement("style")).textContent=${JSON.stringify(CSS_CONTENT)};`;
      return `${DOCBLOCK}\n\n${injectCss}\n\n${code}`;
    },
  };
}
