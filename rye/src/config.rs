use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error};
use toml_edit::Document;

use crate::platform::{get_app_dir, get_latest_cpython_version};
use crate::pyproject::{BuildSystem, SourceRef, SourceRefType};
use crate::sources::PythonVersionRequest;

static CONFIG: Mutex<Option<Arc<Config>>> = Mutex::new(None);

pub fn load() -> Result<(), Error> {
    let cfg_path = get_app_dir().join("config.toml");
    let cfg = if cfg_path.is_file() {
        Config::from_path(&cfg_path)?
    } else {
        Config::default()
    };
    *CONFIG.lock().unwrap() = Some(Arc::new(cfg));
    Ok(())
}

pub struct Config {
    doc: Document,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            doc: Document::new(),
        }
    }
}

impl Config {
    /// Returns the current config
    pub fn current() -> Arc<Config> {
        CONFIG
            .lock()
            .unwrap()
            .as_ref()
            .expect("config not initialized")
            .clone()
    }

    /// Loads a config from a path.
    pub fn from_path(path: &Path) -> Result<Config, Error> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config from '{}'", path.display()))?;
        Ok(Config {
            doc: contents
                .parse::<Document>()
                .with_context(|| format!("failed to parse config from '{}'", path.display()))?,
        })
    }

    /// Returns the default lower bound Python.
    pub fn default_requires_python(&self) -> String {
        match self
            .doc
            .get("default")
            .and_then(|x| x.get("requires-python"))
            .and_then(|x| x.as_str())
        {
            Some(ver) => {
                if ver.trim().parse::<pep440_rs::Version>().is_ok() {
                    format!(">= {}", ver)
                } else {
                    ver.to_string()
                }
            }
            None => ">= 3.8".into(),
        }
    }

    /// Returns the default python toolchain
    pub fn default_toolchain(&self) -> Result<PythonVersionRequest, Error> {
        match self
            .doc
            .get("default")
            .and_then(|x| x.get("toolchain"))
            .and_then(|x| x.as_str())
        {
            Some(ver) => ver.parse(),
            None => get_latest_cpython_version().map(Into::into),
        }
        .context("failed to get default toolchain")
    }

    /// Returns the default build system
    pub fn default_build_system(&self) -> Option<BuildSystem> {
        match self
            .doc
            .get("default")
            .and_then(|x| x.get("build-system"))
            .and_then(|x| x.as_str())
        {
            Some(build_system) => build_system.parse::<BuildSystem>().ok(),
            None => None,
        }
    }

    /// Returns the default license
    pub fn default_license(&self) -> Option<String> {
        self.doc
            .get("default")
            .and_then(|x| x.get("license"))
            .and_then(|x| x.as_str())
            .map(|x| x.to_string())
    }

    /// Pretend that all projects are rye managed.
    pub fn force_rye_managed(&self) -> bool {
        self.doc
            .get("behavior")
            .and_then(|x| x.get("force_rye_managed"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false)
    }

    /// Returns the HTTP proxy that should be used.
    pub fn http_proxy_url(&self) -> Option<String> {
        std::env::var("http_proxy").ok().or_else(|| {
            self.doc
                .get("proxy")
                .and_then(|x| x.get("http"))
                .and_then(|x| x.as_str())
                .map(|x| x.to_string())
        })
    }

    /// Returns the HTTPS proxy that should be used.
    pub fn https_proxy_url(&self) -> Option<String> {
        std::env::var("HTTPS_PROXY")
            .ok()
            .or_else(|| std::env::var("https_proxy").ok())
            .or_else(|| {
                self.doc
                    .get("proxy")
                    .and_then(|x| x.get("https"))
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string())
            })
    }

    /// Returns the list of default sources.
    pub fn sources(&self) -> Result<Vec<SourceRef>, Error> {
        let mut rv = Vec::new();
        let mut need_default = true;
        if let Some(sources) = self.doc.get("sources").and_then(|x| x.as_array_of_tables()) {
            for source in sources {
                let source_ref = SourceRef::from_toml_table(source)?;
                if source_ref.name == "default" {
                    need_default = false;
                }
                rv.push(source_ref);
            }
        }

        if need_default {
            rv.push(SourceRef::from_url(
                "default".to_string(),
                "https://pypi.org/simple/".into(),
                SourceRefType::Index,
            ));
        }

        Ok(rv)
    }
}
