// Copyright (c) Meta Platforms, Inc. and affiliates.

use wasm_bindgen::JsValue;

pub struct WasmJSError(String);
impl WasmJSError {}

impl std::fmt::Display for WasmJSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WasmJSError: {}", self.0)
    }
}

impl std::fmt::Debug for WasmJSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WasmJSError: {}", self.0)
    }
}

impl std::error::Error for WasmJSError {
    fn description(&self) -> &str {
        &self.0
    }
}

impl From<anyhow::Error> for WasmJSError {
    fn from(err: anyhow::Error) -> Self {
        WasmJSError(format!("{:#?}", &err))
    }
}

impl From<WasmJSError> for JsValue {
    fn from(error: WasmJSError) -> JsValue {
        JsValue::from_str(&error.0)
    }
}
