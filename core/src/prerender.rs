use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;

use crate::decode::request::DecodedImage;
use crate::decode::worker::decode_and_resize;
use crate::media::MediaKind;
use crate::project::Project;

const PRERENDER_QUALITY: i32 = 87;

#[derive(Debug, Clone)]
pub struct PrerenderProgress {
    pub done: usize,
    pub total: usize,
    pub rendered: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone)]
pub struct PrerenderOutcome {
    pub rendered: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
}

pub fn cache_dir(target: &Path) -> PathBuf {
    target.join(".marshmallow").join("cache")
}

pub fn clear_cache_dir(target: &Path) -> anyhow::Result<()> {
    let dir = cache_dir(target);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn cache_path_for(target: &Path, source_id: u32, relative_path: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    source_id.hash(&mut hasher);
    relative_path.hash(&mut hasher);
    cache_dir(target).join(format!("{:016x}.jpg", hasher.finish()))
}

pub struct PrerenderEngine;

impl PrerenderEngine {
    // call this from a dedicated background thread, same as CopyEngine::run
    pub fn run(
        project: &Project,
        target_long_edge: u32,
        worker_count: usize,
        progress_tx: &Sender<PrerenderProgress>,
        cancel: &Arc<AtomicBool>,
    ) -> anyhow::Result<PrerenderOutcome> {
        let dir = cache_dir(&project.target);
        std::fs::create_dir_all(&dir)?;

        let jobs: Vec<(PathBuf, PathBuf)> = project
            .items
            .iter()
            .filter(|item| item.kind == MediaKind::Photo)
            .filter_map(|item| {
                let abs = item.absolute_path(&project.sources)?;
                let cache_path =
                    cache_path_for(&project.target, item.source_id, &item.relative_path);
                Some((abs, cache_path))
            })
            .collect();
        let total = jobs.len();
        let jobs = Arc::new(jobs);

        let next = Arc::new(AtomicUsize::new(0));
        let rendered = Arc::new(AtomicUsize::new(0));
        let skipped = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));

        let worker_count = worker_count.max(1);
        let mut handles = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let next = Arc::clone(&next);
            let rendered = Arc::clone(&rendered);
            let skipped = Arc::clone(&skipped);
            let failed = Arc::clone(&failed);
            let cancel = Arc::clone(cancel);
            let progress_tx = progress_tx.clone();

            handles.push(std::thread::spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= jobs.len() {
                    break;
                }
                let (src_path, cache_path) = &jobs[idx];

                if cache_path.exists() {
                    skipped.fetch_add(1, Ordering::Relaxed);
                } else {
                    match render_one(src_path, cache_path, target_long_edge) {
                        Ok(()) => {
                            rendered.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            eprintln!(
                                "marshmallow: prerender failed for {}: {e}",
                                src_path.display()
                            );
                            failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                let rendered_n = rendered.load(Ordering::Relaxed);
                let skipped_n = skipped.load(Ordering::Relaxed);
                let failed_n = failed.load(Ordering::Relaxed);
                let _ = progress_tx.send(PrerenderProgress {
                    done: rendered_n + skipped_n + failed_n,
                    total,
                    rendered: rendered_n,
                    skipped: skipped_n,
                    failed: failed_n,
                });
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }

        Ok(PrerenderOutcome {
            rendered: rendered.load(Ordering::Relaxed),
            skipped: skipped.load(Ordering::Relaxed),
            failed: failed.load(Ordering::Relaxed),
            cancelled: cancel.load(Ordering::Relaxed),
        })
    }
}

fn render_one(src_path: &Path, cache_path: &Path, target_long_edge: u32) -> anyhow::Result<()> {
    let decoded = decode_and_resize(src_path, target_long_edge)?;
    let jpeg_bytes = encode_jpeg(&decoded)?;

    let tmp_path = cache_path.with_extension("jpg.tmp");
    std::fs::write(&tmp_path, &jpeg_bytes)?;
    std::fs::rename(&tmp_path, cache_path)?;
    Ok(())
}

fn encode_jpeg(image: &DecodedImage) -> anyhow::Result<Vec<u8>> {
    let mut compressor = turbojpeg::Compressor::new()?;
    compressor.set_quality(PRERENDER_QUALITY)?;
    let input = turbojpeg::Image {
        pixels: image.rgba.as_slice(),
        width: image.width as usize,
        pitch: image.width as usize * 4,
        height: image.height as usize,
        format: turbojpeg::PixelFormat::RGBA,
    };
    Ok(compressor.compress_to_vec(input)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_is_stable_for_same_identity() {
        let target = Path::new("/target");
        let a = cache_path_for(target, 0, Path::new("DCIM/IMG_0001.JPG"));
        let b = cache_path_for(target, 0, Path::new("DCIM/IMG_0001.JPG"));
        assert_eq!(a, b);
    }

    #[test]
    fn cache_path_differs_by_source_or_relative_path() {
        let target = Path::new("/target");
        let a = cache_path_for(target, 0, Path::new("DCIM/IMG_0001.JPG"));
        let b = cache_path_for(target, 1, Path::new("DCIM/IMG_0001.JPG"));
        let c = cache_path_for(target, 0, Path::new("DCIM/IMG_0002.JPG"));
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn run_renders_a_decodable_cache_file_and_is_idempotent() {
        use crate::media::MediaKind;
        use crate::project::{Decision, MediaItem, Project, Source};

        let tmp = tempfile::tempdir().unwrap();
        let source_dir = tmp.path().join("src");
        std::fs::create_dir_all(source_dir.join("DCIM")).unwrap();
        let photo_path = source_dir.join("DCIM/IMG_0001.JPG");

        let img = image::RgbImage::from_fn(400, 300, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        img.save(&photo_path).unwrap();

        let target_dir = tmp.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();

        let sources = vec![Source {
            id: 0,
            path: source_dir,
        }];
        let items = vec![MediaItem {
            source_id: 0,
            relative_path: PathBuf::from("DCIM/IMG_0001.JPG"),
            kind: MediaKind::Photo,
            size_bytes: std::fs::metadata(&photo_path).unwrap().len(),
            modified: None,
            decision: Decision::Undecided,
            decided_at: None,
        }];
        let project = Project::new(sources, target_dir.clone(), items);

        let (tx, _rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));

        let outcome = PrerenderEngine::run(&project, 2200, 2, &tx, &cancel).unwrap();
        assert_eq!(outcome.rendered, 1);
        assert_eq!(outcome.skipped, 0);
        assert_eq!(outcome.failed, 0);

        let cache_path = cache_path_for(&target_dir, 0, Path::new("DCIM/IMG_0001.JPG"));
        assert!(cache_path.exists());

        let decoded = crate::decode::worker::decode_and_resize(&cache_path, 2200).unwrap();
        assert_eq!(decoded.width, 400);
        assert_eq!(decoded.height, 300);

        let outcome2 = PrerenderEngine::run(&project, 2200, 2, &tx, &cancel).unwrap();
        assert_eq!(outcome2.rendered, 0);
        assert_eq!(outcome2.skipped, 1);
    }
}
