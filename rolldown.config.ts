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
  plugins: [docblock(), dts()],
});

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
