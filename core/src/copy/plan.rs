use std::path::{Path, PathBuf};

/// Destination is namespaced by source basename so two sources with
/// colliding relative paths (e.g. two SD cards both having
/// `DCIM/100CANON/IMG_0001.JPG`) never collide with each other.
pub fn destination_path(target: &Path, source_basename: &str, relative_path: &Path) -> PathBuf {
    target.join(sanitize(source_basename)).join(relative_path)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionResolution {
    /// No file at the destination path yet.
    Copy(PathBuf),
    /// A file with the same size is already there — treated as already copied.
    SkipExisting,
    /// A different file is already there — copy under a suffixed name instead.
    Renamed(PathBuf),
}

pub fn resolve_collision(dest: &Path, src_size: u64) -> std::io::Result<CollisionResolution> {
    if !dest.exists() {
        return Ok(CollisionResolution::Copy(dest.to_path_buf()));
    }
    let existing_size = std::fs::metadata(dest)?.len();
    if existing_size == src_size {
        return Ok(CollisionResolution::SkipExisting);
    }

    let stem = dest
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = dest.extension().map(|e| e.to_string_lossy().to_string());
    let parent = dest.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut n = 1u32;
    loop {
        let candidate_name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return Ok(CollisionResolution::Renamed(candidate));
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn destination_is_namespaced_by_source() {
        let dest = destination_path(
            Path::new("/target"),
            "SDCARD1",
            Path::new("DCIM/IMG_0001.JPG"),
        );
        assert_eq!(dest, PathBuf::from("/target/SDCARD1/DCIM/IMG_0001.JPG"));
    }

    #[test]
    fn no_existing_file_means_plain_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a.jpg");
        assert_eq!(
            resolve_collision(&dest, 10).unwrap(),
            CollisionResolution::Copy(dest)
        );
    }

    #[test]
    fn same_size_existing_file_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a.jpg");
        fs::write(&dest, vec![0u8; 10]).unwrap();
        assert_eq!(
            resolve_collision(&dest, 10).unwrap(),
            CollisionResolution::SkipExisting
        );
    }

    #[test]
    fn different_size_existing_file_gets_renamed() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a.jpg");
        fs::write(&dest, vec![0u8; 5]).unwrap();
        let resolution = resolve_collision(&dest, 10).unwrap();
        assert_eq!(
            resolution,
            CollisionResolution::Renamed(tmp.path().join("a (1).jpg"))
        );
    }
}
