use std::path::Path;

/// Broad category of a media file, derived from its extension.
///
/// Kept extension-based (no content sniffing) — cheap enough to run over
/// tens of thousands of files during a directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Video,
}

const PHOTO_EXTS: &[&str] = &["jpg", "jpeg", "png", "heic", "heif"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "m4v"];

pub fn classify(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if PHOTO_EXTS.contains(&ext.as_str()) {
        Some(MediaKind::Photo)
    } else if VIDEO_EXTS.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_known_extensions() {
        assert_eq!(classify(&PathBuf::from("a.JPG")), Some(MediaKind::Photo));
        assert_eq!(classify(&PathBuf::from("a.heic")), Some(MediaKind::Photo));
        assert_eq!(classify(&PathBuf::from("a.MOV")), Some(MediaKind::Video));
        assert_eq!(classify(&PathBuf::from("a.txt")), None);
        assert_eq!(classify(&PathBuf::from("noext")), None);
    }
}
