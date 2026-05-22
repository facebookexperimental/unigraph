// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;

use crate::FlowGenerator;
use crate::HackGenerator;
use crate::TypeGenGeneratedType;
use crate::TypeScriptGenerator;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    TypeScript,
    Flow,
    Hack,
}

/// Configuration for TypeGen exports
#[derive(Debug, Clone, Deserialize)]
pub struct TypeGenConfigSerialized {
    pub typescript: Option<TypeScriptConfig>,
    pub flow: Option<FlowConfig>,
    pub hack: Option<HackConfig>,
}

pub struct TypeGenConfig {
    pub typescript: Option<TypeScriptConfig>,
    pub flow: Option<FlowConfig>,
    pub hack: Option<HackConfig>,
    pub config_file_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SharedConfig {
    pub export_path: Option<String>,
    pub header: Option<String>,
    pub type_name_prefix: Option<String>,
    pub file_name_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TypeScriptConfig {
    #[serde(flatten)]
    pub shared_config: SharedConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FlowConfig {
    #[serde(flatten)]
    pub shared_config: SharedConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HackConfig {
    #[serde(flatten)]
    pub shared_config: SharedConfig,
}

pub struct TypeGenFile {
    pub path: PathBuf,
    pub content: String,
}

impl TypeGenFile {
    pub fn write(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory for {}", self.path.display())
            })?;
        }
        fs::write(&self.path, &self.content)
            .with_context(|| format!("Failed to write to {}", self.path.display()))?;
        Ok(())
    }
}

impl TypeGenConfig {
    pub fn get_shared_config(&self, lang: Lang) -> Option<&SharedConfig> {
        match lang {
            Lang::TypeScript => self.typescript.as_ref().map(|c| &c.shared_config),
            Lang::Flow => self.flow.as_ref().map(|c| &c.shared_config),
            Lang::Hack => self.hack.as_ref().map(|c| &c.shared_config),
        }
    }

    pub fn get_type_name(&self, original_type_name: &str, lang: Lang) -> String {
        let shared_config = self.get_shared_config(lang);
        if let Some(prefix) = shared_config.and_then(|c| c.type_name_prefix.as_ref()) {
            format!("{}{}", prefix, original_type_name)
        } else {
            original_type_name.to_string()
        }
    }

    pub fn make_file(&self, decl: TypeGenGeneratedType, lang: Lang) -> Result<Option<TypeGenFile>> {
        if let Some(shared_config) = self.get_shared_config(lang) {
            if let Some(export_path) = &shared_config.export_path {
                // Check if this type should be skipped for this language
                if let Some(ref skip) = decl.skip {
                    match lang {
                        Lang::Hack if skip.hack => return Ok(None),
                        Lang::Flow if skip.flow => return Ok(None),
                        Lang::TypeScript if skip.typescript => return Ok(None),
                        _ => {}
                    }
                }

                let content = match lang {
                    Lang::TypeScript => TypeScriptGenerator::generate_typescript(self, &decl),
                    Lang::Flow => FlowGenerator::generate_flow(self, &decl),
                    Lang::Hack => HackGenerator::generate(self, &decl),
                };
                let content = shared_config.prepend_header(content);
                let mut path = self.resolve_path(export_path)?;
                path.push(self.make_file_name(&decl.original_type_name, lang));
                return Ok(Some(TypeGenFile { path, content }));
            }
        }
        Ok(None)
    }

    pub fn make_file_name(&self, type_name: &str, lang: Lang) -> PathBuf {
        let name = match lang {
            Lang::TypeScript => format!("{type_name}.ts"),
            Lang::Flow => format!("{type_name}.js.flow"),
            Lang::Hack => format!("{type_name}.php"),
        };

        if let Some(shared_config) = self.get_shared_config(lang) {
            shared_config.add_file_prefix(&name).into()
        } else {
            name.into()
        }
    }

    /// Make a path relative to this config file's path
    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        let mut resolved_path = self
            .config_file_path
            .clone()
            .parent()
            .with_context(|| {
                format!(
                    "Failed to get parent directory of config file: {}",
                    self.config_file_path.display()
                )
            })?
            .to_owned();
        resolved_path.push(path);
        Ok(resolved_path)
    }
}

impl SharedConfig {
    fn prepend_header(&self, type_content: String) -> String {
        if let Some(header) = &self.header {
            format!("{header}\n\n{type_content}")
        } else {
            type_content
        }
    }

    fn add_file_prefix(&self, file_name: &str) -> String {
        if let Some(prefix) = &self.file_name_prefix {
            format!("{prefix}{file_name}")
        } else {
            file_name.to_string()
        }
    }
}

/// Cache for resolved configs - maps directory paths to configs
static CONFIG_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<TypeGenConfig>>>> = OnceLock::new();

/// Get the configuration for a specific source file path, resolving the closest config
pub fn get_config_for_file<P: AsRef<Path>>(source_file_path: P) -> Result<Arc<TypeGenConfig>> {
    let cache = CONFIG_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let rel_source_path = source_file_path.as_ref();
    let project_root = get_project_root_path();
    // join project root with the relative source path
    let source_path = project_root.join(rel_source_path);

    // If it's a file, get its directory; if it's already a directory, use it as-is
    let source_dir = if source_path.is_file() {
        source_path.parent().unwrap_or(source_path.as_ref())
    } else {
        source_path.as_ref()
    };

    // Check cache first
    {
        let cache_guard = cache.lock().unwrap();
        if let Some(config) = cache_guard.get(source_dir) {
            return Ok(config.clone());
        }
    }

    // Find the closest config file by walking up the directory tree
    let config = Arc::new(
        find_closest_config(source_dir)
            .context("Failed to find closest typegen.toml config file")?,
    );

    // Cache the result
    {
        let mut cache_guard = cache.lock().unwrap();
        cache_guard.insert(source_dir.to_path_buf(), config.clone());
    }

    Ok(config)
}

/// Find the closest typegen.toml config file by walking up the directory tree
fn find_closest_config(start_dir: &Path) -> Result<TypeGenConfig> {
    let mut current_dir = start_dir;
    let mut checked_paths = vec![];

    loop {
        let config_path = current_dir.join("typegen.toml");
        checked_paths.push(config_path.clone());

        if config_path.exists() {
            let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
            let config =
                toml::from_str::<TypeGenConfigSerialized>(&content).with_context(|| {
                    format!("Failed to parse config file: {}", config_path.display())
                })?;
            let config = TypeGenConfig {
                typescript: config.typescript,
                flow: config.flow,
                hack: config.hack,
                config_file_path: config_path.clone(),
            };

            return Ok(config);
        }

        // Move up one directory
        if let Some(parent) = current_dir.parent() {
            current_dir = parent;
        } else {
            // Reached the root, return default config
            break;
        }
    }

    anyhow::bail!(
        "No typegen.toml found in any parent directories of {:?}. Checked paths: {:#?}",
        start_dir,
        checked_paths
    )
}

/// Utility function to write type definition to a file
pub fn write_type_to_file(content: &str, file_path: &Path) -> Result<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {}", file_path.display()))?;
    }
    fs::write(file_path, content)
        .with_context(|| format!("Failed to write to {}", file_path.display()))?;
    Ok(())
}

fn build_system() -> BuildSystem {
    if std::env::var("BUCK2_DAEMON_UUID").is_ok() {
        BuildSystem::Buck2
    } else {
        BuildSystem::Cargo
    }
}

enum BuildSystem {
    /// https://developers.facebook.com/blog/post/2021/07/01/future-of-buck/
    Buck2,
    /// https://github.com/rust-lang/cargo
    Cargo,
}

impl BuildSystem {
    fn is_buck(&self) -> bool {
        match self {
            Self::Buck2 => true,
            Self::Cargo => false,
        }
    }
}

// Project root is the root of the entire project. The project might contain multiple crate and it should not
// be used together with whatever `file!()` macro will return.
pub fn get_project_root_path() -> PathBuf {
    // It seems like when it's built with Buck, PWD will always point to the
    // repo root, regardless of where it's run from. We'll use it as a base dir
    if build_system().is_buck() {
        let pwd = std::env::var("PWD").expect(
            "
`BUCK2_DAEMON_UUID` environment variable was present,
which means this project is being built with buck and relies on `PWD` env
variable to contain the project root, but `PWD` wasn't there",
        );
        return PathBuf::from(pwd);
    }

    // otherwise ask cargo for project root
    let project_root =
        std::env::var("CARGO_MANIFEST_DIR").expect("Can't get project root directory");
    PathBuf::from(project_root)
}
