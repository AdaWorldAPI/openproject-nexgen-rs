//! The application is selected by configuration only.
//!
//! Everything application-specific the compiler needs is in this file's
//! schema: where the source is, which classes form the slice, which method
//! bodies are compiled versus declared as boundaries, and the convention
//! aliases that map an application's own framework idioms (Zammad's
//! `Setting.get`, `X.lookup`) onto polyfill primitives. The compiler's code
//! contains no application names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app: App,
    pub slice: Slice,
    #[serde(default)]
    pub boundary: Boundary,
    /// Ruby call spelling → polyfill primitive. Keys are `Const.method`,
    /// `*.method` (any receiver) or a bare `method`.
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    /// Where the generated crate lives, relative to the config file.
    pub output: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct App {
    pub name: String,
    /// IRI prefix for the SPO harvest.
    pub namespace: String,
    /// Environment variable that names the source checkout.
    pub source_env: String,
    /// Directories that resolve constants, in the application's autoload
    /// order (`app/models`, `app/models/concerns`, `lib`, …).
    pub autoload_roots: Vec<String>,
    /// Migrations whose `create_table` blocks define the schema.
    pub migrations: Vec<String>,
    /// `config.load_defaults` of the application (R-LOAD-DEFAULTS).
    pub rails_load_defaults: Option<String>,
    /// Superclasses that end the inheritance walk at ActiveRecord itself.
    #[serde(default = "default_framework_roots")]
    pub framework_roots: Vec<String>,
}

fn default_framework_roots() -> Vec<String> {
    vec!["ActiveRecord::Base".into(), "ApplicationRecord".into()]
}

#[derive(Debug, Clone, Deserialize)]
pub struct Slice {
    pub models: Vec<String>,
    /// File listing the callback methods whose bodies are compiled and run;
    /// shared verbatim with the behavioural oracle.
    pub verbatim_file: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Boundary {
    /// Calls that leave the slice and are answered by the host, in Ruby
    /// spelling: `Const.method` for class methods, `Const#method` for
    /// instance methods.
    #[serde(default)]
    pub calls: Vec<String>,
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub base: PathBuf,
    pub source: PathBuf,
    pub verbatim: Vec<String>,
}

impl Loaded {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let config: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let base = path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let source = std::env::var(&config.app.source_env)
            .map(PathBuf::from)
            .map_err(|_| format!("{} is unset — point it at the {} checkout", config.app.source_env, config.app.name))?;
        let vpath = base.join(&config.slice.verbatim_file);
        let verbatim = std::fs::read_to_string(&vpath)
            .map_err(|e| format!("{}: {e}", vpath.display()))?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect();
        Ok(Self {
            config,
            base,
            source,
            verbatim,
        })
    }

    #[must_use]
    pub fn alias_for(&self, recv: Option<&str>, method: &str) -> Option<&str> {
        let a = &self.config.aliases;
        recv.and_then(|r| a.get(&format!("{r}.{method}")))
            .or_else(|| a.get(&format!("*.{method}")))
            .or_else(|| if recv.is_none() { a.get(method) } else { None })
            .map(String::as_str)
    }

    #[must_use]
    pub fn is_boundary(&self, spelling: &str) -> bool {
        self.config.boundary.calls.iter().any(|c| c == spelling)
    }
}
