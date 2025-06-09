// Copyright (c) Meta Platforms, Inc. and affiliates.

pub struct UnigraphError(String);
impl UnigraphError {}

impl std::fmt::Display for UnigraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UnigraphError: {}", self.0)
    }
}

impl std::fmt::Debug for UnigraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UnigraphError: {}", self.0)
    }
}

impl std::error::Error for UnigraphError {
    fn description(&self) -> &str {
        &self.0
    }
}

impl From<anyhow::Error> for UnigraphError {
    fn from(err: anyhow::Error) -> Self {
        UnigraphError(format!("{:#?}", &err))
    }
}
