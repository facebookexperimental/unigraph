// Copyright (c) Meta Platforms, Inc. and affiliates.

use unigraph_error::UnigraphError;
use unigraph_error::into_unigraph_error;
use wasm_bindgen::JsError;
use wasm_bindgen::JsValue;

pub struct WasmJSError(UnigraphError);
impl WasmJSError {}

impl std::fmt::Display for WasmJSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::fmt::Debug for WasmJSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::error::Error for WasmJSError {}

impl From<anyhow::Error> for WasmJSError {
    fn from(err: anyhow::Error) -> Self {
        WasmJSError(into_unigraph_error(&err.context("WasmJS Error")))
    }
}

impl From<WasmJSError> for JsValue {
    fn from(error: WasmJSError) -> JsValue {
        JsError::new(&format!("{error:#?}")).into()
    }
}
