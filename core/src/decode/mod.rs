pub mod cache;
pub mod request;
pub mod worker;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::{bounded, unbounded, Receiver, Select, Sender};

pub use cache::DecodedCache;
pub use request::{DecodeJob, DecodeResult, DecodedImage};

/// Background decode pool feeding a bounded look-ahead window of photos.
/// Workers only ever produce plain `Send` pixel data — GTK textures are
/// built from that on the UI thread, never here.
pub struct DecodePipeline {
    priority_tx: Sender<DecodeJob>,
    prefetch_tx: Sender<DecodeJob>,
    pub result_rx: Receiver<DecodeResult>,
    generation: Arc<AtomicU64>,
    target_long_edge: Arc<AtomicU32>,
}

/// Leaves roughly one core free for the compositor/UI thread rather than
/// saturating every core with decode work.
pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .saturating_sub(1)
        .clamp(2, 8)
}

impl DecodePipeline {
    pub fn new(worker_count: usize, target_long_edge: u32) -> Self {
        let worker_count = worker_count.max(1);
        let (priority_tx, priority_rx) = bounded::<DecodeJob>(4);
        // Must comfortably exceed the largest look-ahead window a caller
        // configures (app/src/state.rs uses ~105) or most of the window
        // would silently fail to even get queued via `try_send`.
        let (prefetch_tx, prefetch_rx) = bounded::<DecodeJob>(256);
        let (result_tx, result_rx) = unbounded::<DecodeResult>();
        let generation = Arc::new(AtomicU64::new(0));
        let target_long_edge = Arc::new(AtomicU32::new(target_long_edge));

        for _ in 0..worker_count {
            spawn_worker(
                priority_rx.clone(),
                prefetch_rx.clone(),
                result_tx.clone(),
                Arc::clone(&generation),
                Arc::clone(&target_long_edge),
            );
        }

        Self {
            priority_tx,
            prefetch_tx,
            result_rx,
            generation,
            target_long_edge,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Invalidate in-flight results for the previous window. Call this on
    /// any non-contiguous jump (e.g. jump-to-undecided) so stale decodes
    /// arriving late are dropped instead of polluting the cache.
    pub fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn set_target_long_edge(&self, size: u32) {
        self.target_long_edge.store(size, Ordering::Relaxed);
    }

    /// The exact item on screen but not yet decoded — jumps the queue.
    pub fn request_priority(&self, index: usize, path: PathBuf) {
        let job = DecodeJob {
            index,
            path,
            generation: self.generation(),
        };
        let _ = self.priority_tx.try_send(job);
    }

    /// A look-ahead item — best effort, dropped silently if the queue is full.
    pub fn request_prefetch(&self, index: usize, path: PathBuf) {
        let job = DecodeJob {
            index,
            path,
            generation: self.generation(),
        };
        let _ = self.prefetch_tx.try_send(job);
    }
}

fn spawn_worker(
    priority_rx: Receiver<DecodeJob>,
    prefetch_rx: Receiver<DecodeJob>,
    result_tx: Sender<DecodeResult>,
    generation: Arc<AtomicU64>,
    target_long_edge: Arc<AtomicU32>,
) {
    std::thread::spawn(move || loop {
        let job = match priority_rx.try_recv() {
            Ok(job) => Some(job),
            Err(_) => {
                let mut sel = Select::new();
                let pi = sel.recv(&priority_rx);
                let pf = sel.recv(&prefetch_rx);
                let oper = sel.select();
                match oper.index() {
                    i if i == pi => oper.recv(&priority_rx).ok(),
                    i if i == pf => oper.recv(&prefetch_rx).ok(),
                    _ => None,
                }
            }
        };

        let Some(job) = job else { break };

        if job.generation != generation.load(Ordering::Relaxed) {
            continue;
        }

        let target = target_long_edge.load(Ordering::Relaxed);
        let outcome = worker::decode_and_resize(&job.path, target).map_err(|e| e.to_string());
        let result = DecodeResult {
            index: job.index,
            generation: job.generation,
            outcome,
        };
        if result_tx.send(result).is_err() {
            break;
        }
    });
}

/// Compute the look-ahead/behind window around `current`, clamped to
/// `[0, len)`. Forward-biased since review is normally a forward march.
pub fn window_around(
    current: usize,
    len: usize,
    behind: usize,
    ahead: usize,
) -> std::ops::Range<usize> {
    if len == 0 {
        return 0..0;
    }
    let start = current.saturating_sub(behind);
    let end = (current + ahead + 1).min(len);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_clamps_to_bounds() {
        assert_eq!(window_around(0, 10, 2, 8), 0..9);
        assert_eq!(window_around(9, 10, 2, 8), 7..10);
        assert_eq!(window_around(5, 10, 2, 8), 3..10);
    }
}
