pub mod log;
pub mod plan;

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Sender;

use crate::project::{Decision, Project};
use crate::scan::source_basename;
use log::{CopyLog, CopyLogEntry, CopyStatus, CopySummary};
use plan::{destination_path, resolve_collision, CollisionResolution};

const CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CopyProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<PathBuf>,
    pub current_file_bytes_done: u64,
    pub current_file_bytes_total: u64,
}

#[derive(Debug, Clone)]
pub struct CopyOutcome {
    pub files_copied: usize,
    pub files_skipped: usize,
    pub files_renamed: usize,
    pub files_failed: usize,
    pub bytes_copied: u64,
    pub log_path: PathBuf,
    pub cancelled: bool,
}

enum CopyFileOutcome {
    Copied(PathBuf),
    Renamed(PathBuf),
    SkippedExisting(PathBuf),
    Cancelled,
}

pub struct CopyEngine;

impl CopyEngine {
    // call this from a dedicated background thread and forward CopyProgress over progress_tx to the UI
    pub fn run(
        project: &Project,
        progress_tx: &Sender<CopyProgress>,
        cancel: &Arc<AtomicBool>,
    ) -> anyhow::Result<CopyOutcome> {
        let kept: Vec<_> = project
            .items
            .iter()
            .filter(|i| i.decision == Decision::Keep)
            .collect();
        let files_total = kept.len();
        let bytes_total: u64 = kept.iter().map(|i| i.size_bytes).sum();

        let (mut log, log_path) = CopyLog::create(&project.target)?;
        let start = Instant::now();

        let mut files_done = 0usize;
        let mut bytes_done = 0u64;
        let mut files_copied = 0usize;
        let mut files_skipped = 0usize;
        let mut files_renamed = 0usize;
        let mut files_failed = 0usize;
        let mut cancelled = false;

        for item in kept {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }

            let Some(source) = project.sources.iter().find(|s| s.id == item.source_id) else {
                files_failed += 1;
                files_done += 1;
                continue;
            };
            let src_path = source.path.join(&item.relative_path);
            let dest_base = destination_path(
                &project.target,
                &source_basename(&source.path),
                &item.relative_path,
            );

            let base_progress = CopyProgress {
                files_done,
                files_total,
                bytes_done,
                bytes_total,
                current_file: Some(item.relative_path.clone()),
                current_file_bytes_done: 0,
                current_file_bytes_total: item.size_bytes,
            };
            let _ = progress_tx.send(base_progress.clone());

            let file_start = Instant::now();
            let result = copy_one(
                &src_path,
                &dest_base,
                item.size_bytes,
                cancel,
                progress_tx,
                &base_progress,
            );
            let duration_ms = file_start.elapsed().as_millis() as u64;

            match result {
                Ok(CopyFileOutcome::Copied(dest)) => {
                    files_copied += 1;
                    bytes_done += item.size_bytes;
                    log_entry(
                        &mut log,
                        &src_path,
                        &dest,
                        item.size_bytes,
                        CopyStatus::Copied,
                        None,
                        duration_ms,
                    )?;
                }
                Ok(CopyFileOutcome::Renamed(dest)) => {
                    files_renamed += 1;
                    bytes_done += item.size_bytes;
                    log_entry(
                        &mut log,
                        &src_path,
                        &dest,
                        item.size_bytes,
                        CopyStatus::Renamed,
                        None,
                        duration_ms,
                    )?;
                }
                Ok(CopyFileOutcome::SkippedExisting(dest)) => {
                    files_skipped += 1;
                    log_entry(
                        &mut log,
                        &src_path,
                        &dest,
                        item.size_bytes,
                        CopyStatus::SkippedExisting,
                        None,
                        duration_ms,
                    )?;
                }
                Ok(CopyFileOutcome::Cancelled) => {
                    cancelled = true;
                }
                Err(e) => {
                    files_failed += 1;
                    log_entry(
                        &mut log,
                        &src_path,
                        &dest_base,
                        item.size_bytes,
                        CopyStatus::Error,
                        Some(e.to_string()),
                        duration_ms,
                    )?;
                }
            }

            files_done += 1;
            if cancelled {
                break;
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        log.append_summary(&CopySummary {
            summary: true,
            files_copied,
            files_skipped,
            files_renamed,
            files_failed,
            bytes_copied: bytes_done,
            elapsed_ms,
        })?;

        let _ = progress_tx.send(CopyProgress {
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            current_file: None,
            current_file_bytes_done: 0,
            current_file_bytes_total: 0,
        });

        Ok(CopyOutcome {
            files_copied,
            files_skipped,
            files_renamed,
            files_failed,
            bytes_copied: bytes_done,
            log_path,
            cancelled,
        })
    }
}

fn log_entry(
    log: &mut CopyLog,
    src: &std::path::Path,
    dest: &std::path::Path,
    size_bytes: u64,
    status: CopyStatus,
    error_message: Option<String>,
    duration_ms: u64,
) -> anyhow::Result<()> {
    log.append_entry(&CopyLogEntry {
        timestamp: chrono::Utc::now(),
        source_path: src.to_path_buf(),
        dest_path: dest.to_path_buf(),
        size_bytes,
        status,
        error_message,
        duration_ms,
    })
}

fn copy_one(
    src: &std::path::Path,
    dest_base: &std::path::Path,
    size: u64,
    cancel: &Arc<AtomicBool>,
    progress_tx: &Sender<CopyProgress>,
    base: &CopyProgress,
) -> anyhow::Result<CopyFileOutcome> {
    let (dest, was_renamed) = match resolve_collision(dest_base, size)? {
        CollisionResolution::SkipExisting => {
            return Ok(CopyFileOutcome::SkippedExisting(dest_base.to_path_buf()))
        }
        CollisionResolution::Copy(dest) => (dest, false),
        CollisionResolution::Renamed(dest) => (dest, true),
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut src_file = File::open(src)?;
    let mut dest_file = File::create(&dest)?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut copied = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(dest_file);
            let _ = std::fs::remove_file(&dest);
            return Ok(CopyFileOutcome::Cancelled);
        }
        let n = src_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dest_file.write_all(&buf[..n])?;
        copied += n as u64;

        let _ = progress_tx.send(CopyProgress {
            bytes_done: base.bytes_done + copied,
            current_file_bytes_done: copied,
            current_file_bytes_total: size,
            ..base.clone()
        });
    }
    dest_file.sync_all()?;

    if was_renamed {
        Ok(CopyFileOutcome::Renamed(dest))
    } else {
        Ok(CopyFileOutcome::Copied(dest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaKind;
    use crate::project::{MediaItem, Source};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn copies_kept_items_and_skips_undecided_and_trashed() {
        let tmp = tempfile::tempdir().unwrap();
        let source_dir = tmp.path().join("src");
        fs::create_dir_all(source_dir.join("DCIM")).unwrap();
        fs::write(source_dir.join("DCIM/keep.jpg"), b"kept-bytes").unwrap();
        fs::write(source_dir.join("DCIM/trash.jpg"), b"trashed-bytes").unwrap();
        let target_dir = tmp.path().join("target");
        fs::create_dir_all(&target_dir).unwrap();

        let sources = vec![Source {
            id: 0,
            path: source_dir.clone(),
        }];
        let items = vec![
            MediaItem {
                source_id: 0,
                relative_path: PathBuf::from("DCIM/keep.jpg"),
                kind: MediaKind::Photo,
                size_bytes: fs::metadata(source_dir.join("DCIM/keep.jpg"))
                    .unwrap()
                    .len(),
                modified: None,
                decision: Decision::Keep,
                decided_at: None,
            },
            MediaItem {
                source_id: 0,
                relative_path: PathBuf::from("DCIM/trash.jpg"),
                kind: MediaKind::Photo,
                size_bytes: fs::metadata(source_dir.join("DCIM/trash.jpg"))
                    .unwrap()
                    .len(),
                modified: None,
                decision: Decision::Trash,
                decided_at: None,
            },
        ];
        let project = Project::new(sources, target_dir.clone(), items);

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let outcome = CopyEngine::run(&project, &tx, &cancel).unwrap();

        assert_eq!(outcome.files_copied, 1);
        assert!(target_dir.join("src/DCIM/keep.jpg").exists());
        assert!(!target_dir.join("src/DCIM/trash.jpg").exists());
        assert!(outcome.log_path.exists());
    }
}
