# Unigraph

> **Experimental.** This repo is not production-ready. APIs and structure may change or disappear without notice.

Graph visualizations built with Rust + WebAssembly + WebGL on an open-source web stack.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
brew install llvm          # required for wasm zstd builds on macOS
pnpm install
```

Set LLVM env vars (add to your shell profile):

```bash
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
export CC=/opt/homebrew/opt/llvm/bin/clang
export AR=/opt/homebrew/opt/llvm/bin/llvm-ar
```

## Quick Start

```bash
ut build wasm    # compile wasm artifact
ut dev           # start tailwind + rolldown watchers
ut serve         # start rust web server
```

## License

MIT — see [LICENSE](./LICENSE).
