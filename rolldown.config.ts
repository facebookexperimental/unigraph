import { readFileSync } from "node:fs";
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
  plugins: [injectCSS(), dts()],
});

const DOCBLOCK = `/**
 * @oncall unigraph
 * @nolint
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
