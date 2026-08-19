use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DecodeJob {
    pub index: usize,
    pub path: PathBuf,
    pub generation: u64,
}

/// Plain, `Send`-only decoded pixel data. GTK/GDK textures are constructed
/// from this on the UI thread only — worker threads never touch GObjects.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8 (stride == width * 4).
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    pub fn approx_bytes(&self) -> usize {
        self.rgba.len()
    }
}

#[derive(Debug)]
pub struct DecodeResult {
    pub index: usize,
    pub generation: u64,
    pub outcome: Result<DecodedImage, String>,
}
