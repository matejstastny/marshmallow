use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::media::MediaKind;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Undecided,
    Keep,
    Trash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: u32,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub source_id: u32,
    pub relative_path: PathBuf,
    pub kind: MediaKind,
    pub size_bytes: u64,
    pub modified: Option<DateTime<Utc>>,
    pub decision: Decision,
    pub decided_at: Option<DateTime<Utc>>,
}

impl MediaItem {
    /// Absolute path on disk, given the source list it belongs to.
    pub fn absolute_path(&self, sources: &[Source]) -> Option<PathBuf> {
        sources
            .iter()
            .find(|s| s.id == self.source_id)
            .map(|s| s.path.join(&self.relative_path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sources: Vec<Source>,
    pub target: PathBuf,
    pub items: Vec<MediaItem>,
}

impl Project {
    pub fn new(sources: Vec<Source>, target: PathBuf, items: Vec<MediaItem>) -> Self {
        let now = Utc::now();
        Self {
            schema_version: SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
            sources,
            target,
            items,
        }
    }

    /// Default project file location: it travels with the target drive.
    pub fn default_path(target: &Path) -> PathBuf {
        target.join(".marshmallow").join("project.json")
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }

    /// Atomically write the project file: write to a sibling `.tmp`, fsync,
    /// then rename over the real path. A crash mid-write leaves the
    /// previous good file intact rather than a half-written one. The prior
    /// good version is preserved as `.bak` as a cheap extra safety net.
    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        self.updated_at = Utc::now();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let tmp_path = path.with_extension("json.tmp");
        let data = serde_json::to_string_pretty(self)?;
        {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(data.as_bytes())?;
            f.sync_all()?;
        }

        if path.exists() {
            let bak_path = path.with_extension("json.bak");
            let _ = fs::copy(path, bak_path);
        }

        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    pub fn keep_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.decision == Decision::Keep)
            .count()
    }

    pub fn trash_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.decision == Decision::Trash)
            .count()
    }

    pub fn undecided_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.decision == Decision::Undecided)
            .count()
    }

    pub fn kept_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.decision == Decision::Keep)
            .map(|i| i.size_bytes)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project(dir: &Path) -> Project {
        let sources = vec![Source {
            id: 0,
            path: dir.join("src1"),
        }];
        let items = vec![MediaItem {
            source_id: 0,
            relative_path: PathBuf::from("a.jpg"),
            kind: MediaKind::Photo,
            size_bytes: 100,
            modified: None,
            decision: Decision::Undecided,
            decided_at: None,
        }];
        Project::new(sources, dir.join("target"), items)
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = Project::default_path(tmp.path());
        let mut project = sample_project(tmp.path());
        project.save(&path).unwrap();

        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].relative_path, PathBuf::from("a.jpg"));
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn save_leaves_previous_version_as_bak() {
        let tmp = tempfile::tempdir().unwrap();
        let path = Project::default_path(tmp.path());
        let mut project = sample_project(tmp.path());
        project.save(&path).unwrap();
        project.items[0].decision = Decision::Keep;
        project.save(&path).unwrap();

        let bak_path = path.with_extension("json.bak");
        assert!(bak_path.exists());
        let bak = Project::load(&bak_path).unwrap();
        assert_eq!(bak.items[0].decision, Decision::Undecided);
    }

    #[test]
    fn counts_are_correct() {
        let tmp = tempfile::tempdir().unwrap();
        let mut project = sample_project(tmp.path());
        project.items.push(MediaItem {
            source_id: 0,
            relative_path: PathBuf::from("b.jpg"),
            kind: MediaKind::Photo,
            size_bytes: 50,
            modified: None,
            decision: Decision::Keep,
            decided_at: None,
        });
        assert_eq!(project.keep_count(), 1);
        assert_eq!(project.undecided_count(), 1);
        assert_eq!(project.kept_bytes(), 50);
    }
}
