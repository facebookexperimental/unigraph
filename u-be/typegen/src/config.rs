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

/// Configuration for TypeGen exports
#[derive(Debug, Clone, Deserialize)]
pub struct TypeGenConfigSerialized {
    pub typescript: Option<TypeScriptConfig>,
    pub flow: Option<FlowConfig>,
}

pub struct TypeGenConfig {
    pub typescript: Option<TypeScriptConfig>,
    pub flow: Option<FlowConfig>,
    pub config_file_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SharedConfig {
    pub export_path: Option<String>,
    pub header: Option<String>,
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

impl TypeGenConfig {
    /// Make a path relative to this config file's path
    pub fn resolve_path(&self, path: &str) -> Result<PathBuf> {
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
