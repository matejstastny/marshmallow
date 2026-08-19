use std::path::Path;

use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::media::classify;
use crate::project::{Decision, MediaItem, Source};

/// Recursively scan every source directory and build the flat item list
/// for a new project. Existing decisions from a prior scan of the same
/// (source_id, relative_path) are preserved by the caller via `merge`.
pub fn scan_sources(sources: &[Source]) -> Vec<MediaItem> {
    let mut items = Vec::new();
    for source in sources {
        for entry in WalkDir::new(&source.path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(kind) = classify(entry.path()) else {
                continue;
            };
            let Ok(relative_path) = entry.path().strip_prefix(&source.path) else {
                continue;
            };
            let metadata = entry.metadata().ok();
            let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = metadata
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from);

            items.push(MediaItem {
                source_id: source.id,
                relative_path: relative_path.to_path_buf(),
                kind,
                size_bytes,
                modified,
                decision: Decision::Undecided,
                decided_at: None,
            });
        }
    }
    items.sort_by(|a, b| (a.source_id, &a.relative_path).cmp(&(b.source_id, &b.relative_path)));
    items
}

/// Merge a fresh scan with a previous item list, carrying decisions
/// forward for items whose (source_id, relative_path) identity matches.
/// Items present in `previous` but no longer found on disk are dropped.
pub fn merge_with_previous(fresh: Vec<MediaItem>, previous: &[MediaItem]) -> Vec<MediaItem> {
    fresh
        .into_iter()
        .map(|mut item| {
            if let Some(prev) = previous
                .iter()
                .find(|p| p.source_id == item.source_id && p.relative_path == item.relative_path)
            {
                item.decision = prev.decision;
                item.decided_at = prev.decided_at;
            }
            item
        })
        .collect()
}

pub fn next_source_id(sources: &[Source]) -> u32 {
    sources
        .iter()
        .map(|s| s.id)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0)
}

pub fn source_basename(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_only_known_media_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.jpg"), b"x").unwrap();
        fs::write(tmp.path().join("b.mov"), b"x").unwrap();
        fs::write(tmp.path().join("c.txt"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/d.png"), b"x").unwrap();

        let sources = vec![Source {
            id: 0,
            path: tmp.path().to_path_buf(),
        }];
        let items = scan_sources(&sources);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn merge_preserves_decisions_by_identity() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.jpg"), b"x").unwrap();
        let sources = vec![Source {
            id: 0,
            path: tmp.path().to_path_buf(),
        }];

        let mut previous = scan_sources(&sources);
        previous[0].decision = Decision::Keep;

        let fresh = scan_sources(&sources);
        let merged = merge_with_previous(fresh, &previous);
        assert_eq!(merged[0].decision, Decision::Keep);
    }
}
