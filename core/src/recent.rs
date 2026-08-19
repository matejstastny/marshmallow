use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub target: PathBuf,
}

impl RecentProject {
    fn config_path() -> PathBuf {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        config_home.join("marshmallow").join("recent.json")
    }

    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(Self::config_path()).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(target: &Path) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let recent = RecentProject {
            target: target.to_path_buf(),
        };
        std::fs::write(path, serde_json::to_string_pretty(&recent)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_is_stable_and_nonempty() {
        let path = RecentProject::config_path();
        assert!(path.to_string_lossy().contains("marshmallow"));
    }
}
