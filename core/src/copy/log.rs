use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyStatus {
    Copied,
    SkippedExisting,
    Renamed,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyLogEntry {
    pub timestamp: DateTime<Utc>,
    pub source_path: PathBuf,
    pub dest_path: PathBuf,
    pub size_bytes: u64,
    pub status: CopyStatus,
    pub error_message: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopySummary {
    pub summary: bool,
    pub files_copied: usize,
    pub files_skipped: usize,
    pub files_renamed: usize,
    pub files_failed: usize,
    pub bytes_copied: u64,
    pub elapsed_ms: u64,
}

/// Appends one JSON object per line, flushed immediately after every
/// write so the log stays readable even if the process crashes mid-copy.
pub struct CopyLog {
    writer: BufWriter<File>,
}

impl CopyLog {
    pub fn create(target: &Path) -> anyhow::Result<(Self, PathBuf)> {
        let dir = target.join(".marshmallow").join("logs");
        std::fs::create_dir_all(&dir)?;
        let filename = format!("copy-{}.jsonl", Utc::now().format("%Y%m%dT%H%M%SZ"));
        let path = dir.join(filename);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok((
            Self {
                writer: BufWriter::new(file),
            },
            path,
        ))
    }

    pub fn append_entry(&mut self, entry: &CopyLogEntry) -> anyhow::Result<()> {
        self.write_line(entry)
    }

    pub fn append_summary(&mut self, summary: &CopySummary) -> anyhow::Result<()> {
        self.write_line(summary)
    }

    fn write_line<T: Serialize>(&mut self, value: &T) -> anyhow::Result<()> {
        let line = serde_json::to_string(value)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        Ok(())
    }
}
