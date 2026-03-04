# Unigraph

This is a standalone experiment of Unigraph using open source web stack.
The goal of this project is to implement graph visualizations using Rust + Wasm + WebGL
and potentially open source it later
see https://fburl.com/gdoc/1q1oex39

This is an experiment. If you find this file a year from now please
feel free to nuke the whole thing

## Install the target
> rustup target add wasm32-unknown-unknown

## install wasm-bindgen
> cargo install wasm-bindgen-cli

## install uv
> cargo install uv

## install llvm
by default on macs wasm won't be able to build `zstd`. to fix it you can install llvm
> brew install llvm

and set env variables:
```
export PATH="/opt/homebrew/opt/llvm/bin/:$PATH"
export CC=/opt/homebrew/opt/llvm/bin/clang
export AR=/opt/homebrew/opt/llvm/bin/llvm-ar

# for fish: config.fish
set -x PATH /opt/homebrew/opt/llvm/bin $PATH
set -x CC /opt/homebrew/opt/llvm/bin/clang
set -x AR /opt/homebrew/opt/llvm/bin/llvm-ar
```

see https://github.com/gyscos/zstd-rs/issues/93#issuecomment-2110684816

## Build wasm target (this will create a wasm artifact)
> ./bin/build_wasm

## instal node_modules
> pnpm i

## Run tailwind + rollup watcher that will build JS bundles
> pnpm run dev

## Run rust webserver that will pring the localhost URL
> cargo run serve --file-path ./sample_graph.json

## License
Unigraph is MIT licensed, as found in the LICENSE file.
