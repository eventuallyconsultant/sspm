use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
  #[serde(default)]
  pub profiles: HashMap<String, Vec<String>>,
  #[serde(default)]
  pub processes: IndexMap<String, ProcessDef>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProcessDef {
  pub name: Option<String>,
  pub command: String,
}

impl ProcessDef {
  pub fn display_name<'a>(&'a self, key: &'a str) -> &'a str {
    self.name.as_deref().unwrap_or(key)
  }
}

impl Config {
  pub fn load(path: &str) -> anyhow::Result<Self> {
    let content = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path, e))?;
    let config: Config = toml::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse TOML: {}", e))?;
    Ok(config)
  }

  pub fn profile_processes(&self, profile: &str) -> anyhow::Result<Vec<String>> {
    Ok(self.profiles.get(profile).cloned().unwrap_or_default())
  }

  /// Returns all process keys in declaration order.
  pub fn ordered_keys(&self) -> Vec<String> {
    self.processes.keys().cloned().collect()
  }
}
