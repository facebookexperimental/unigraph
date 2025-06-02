// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::borrow::Cow;

use anyhow::Result;

pub async fn create_shader(code: &str, device: &wgpu::Device) -> Result<wgpu::ShaderModule> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(code)),
    });

    let compilation_info = shader.get_compilation_info().await;

    for message in compilation_info.messages {
        let t = match message.message_type {
            wgpu::CompilationMessageType::Error => "Error",
            wgpu::CompilationMessageType::Warning => "Warning",
            wgpu::CompilationMessageType::Info => "Info",
        };
        log_info(format!(
            "[{}] Shader compilation message: {}",
            t, message.message
        ));
    }
    Ok(shader)
}

pub fn log_info<S: std::fmt::Display>(msg: S) {
    #[cfg(target_arch = "wasm32")]
    {
        log::info!("{}", msg);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("{}", msg);
    }
}
