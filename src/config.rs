use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
  pub profiles: HashMap<String, Vec<String>>,
  pub processes: HashMap<String, ProcessDef>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProcessDef {
  pub name: String,
  pub command: String,
}

impl Config {
  pub fn load(path: &str) -> anyhow::Result<Self> {
    let content = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path, e))?;
    let config: Config = toml::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse TOML: {}", e))?;
    Ok(config)
  }

  pub fn profile_processes(&self, profile: &str) -> anyhow::Result<Vec<String>> {
    self
      .profiles
      .get(profile)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found in config", profile))
  }

  /// Returns all process keys in a stable order (sorted alphabetically).
  pub fn ordered_keys(&self) -> Vec<String> {
    let mut keys: Vec<String> = self.processes.keys().cloned().collect();
    keys.sort();
    keys
  }
}
