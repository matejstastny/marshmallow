use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DecodeJob {
    pub index: usize,
    pub path: PathBuf,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
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
