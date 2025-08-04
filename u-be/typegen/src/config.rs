use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use serde::Deserialize;

/// Configuration for TypeGen exports
#[derive(Debug, Clone, Deserialize)]
pub struct TypeGenConfig {
    #[serde(default)]
    pub typescript: TypeScriptConfig,
    #[serde(default)]
    pub flow: FlowConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TypeScriptConfig {
    pub export_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FlowConfig {
    pub export_path: Option<String>,
}

impl Default for TypeGenConfig {
    fn default() -> Self {
        Self {
            typescript: TypeScriptConfig {
                export_path: env::var("TS_EXPORT_PATH").ok(),
            },
            flow: FlowConfig {
                export_path: env::var("FLOW_EXPORT_PATH").ok(),
            },
        }
    }
}

// Cache for resolved configs - maps directory paths to configs
static CONFIG_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<TypeGenConfig>>>> = OnceLock::new();

/// Get the configuration for a specific source file path, resolving the closest config
pub fn get_config_for_file<P: AsRef<Path>>(source_file_path: P) -> Arc<TypeGenConfig> {
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
            return config.clone();
        }
    }

    // Find the closest config file by walking up the directory tree
    let config = find_closest_config(source_dir);

    // Cache the result
    {
        let mut cache_guard = cache.lock().unwrap();
        cache_guard.insert(source_dir.to_path_buf(), config.clone());
    }

    config
}

/// Get the global TypeGen configuration (fallback for backwards compatibility)
pub fn get_config() -> &'static TypeGenConfig {
    // For backwards compatibility, use the current directory as the source
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = get_config_for_file(&current_dir);

    // We need to leak this to return a static reference
    // This is safe because we're only using it for backwards compatibility
    Box::leak(Box::new((*config).clone()))
}

/// Find the closest typegen.toml config file by walking up the directory tree
fn find_closest_config(start_dir: &Path) -> Arc<TypeGenConfig> {
    let mut current_dir = start_dir;

    loop {
        let config_path = current_dir.join("typegen.toml");

        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(mut config) = toml::from_str::<TypeGenConfig>(&content) {
                    // Resolve paths relative to the config file location
                    if let Some(ref export_path) = config.typescript.export_path {
                        let resolved_path = current_dir.join(export_path);
                        config.typescript.export_path =
                            Some(resolved_path.to_string_lossy().to_string());
                    }

                    if let Some(ref export_path) = config.flow.export_path {
                        let resolved_path = current_dir.join(export_path);
                        config.flow.export_path = Some(resolved_path.to_string_lossy().to_string());
                    }

                    return Arc::new(config);
                }
            }
        }

        // Move up one directory
        if let Some(parent) = current_dir.parent() {
            current_dir = parent;
        } else {
            // Reached the root, return default config
            break;
        }
    }

    // No config file found, return default
    Arc::new(TypeGenConfig::default())
}

/// Utility function to write type definition to a file
pub fn write_type_to_file(content: &str, file_path: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = std::path::Path::new(file_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file_path, content)?;
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
