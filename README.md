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

## Build wasm target
> ./bin/build_wasm

## instal node_modules
> pnpm i

## Bundle JS and start a web server
> pnpm run serve


## Export rust types to TypeScript
> cargo test export_bindings

## To sync Rust<->TS types
> cargo test export_bindings

## License
Unigraph is MIT licensed, as found in the LICENSE file.
