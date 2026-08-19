use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use marshmallow_core::decode::{DecodePipeline, DecodedCache};
use marshmallow_core::project::{Decision, Project, Source};

pub const DECODE_TARGET_LONG_EDGE: u32 = 2200;

// if you lower WINDOW_AHEAD for a RAM-constrained board, shrink CACHE_BYTE_BUDGET/CACHE_CAPACITY to match
pub const CACHE_CAPACITY: usize = 130;
pub const CACHE_BYTE_BUDGET: usize = 1792 * 1024 * 1024;
pub const WINDOW_BEHIND: usize = 5;
pub const WINDOW_AHEAD: usize = 100;

pub struct AppState {
    pub sources_pending: Vec<Source>,
    pub target_pending: Option<PathBuf>,
    pub project: Option<Project>,
    pub project_path: Option<PathBuf>,
    pub pipeline: Option<DecodePipeline>,
    pub cache: DecodedCache,
    pub current_index: usize,
    pub history: Vec<(usize, Decision)>,
    pub saver: Option<ProjectSaver>,
    pub copy_cancel: Option<Arc<AtomicBool>>,
    pub prerender_cancel: Option<Arc<AtomicBool>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sources_pending: Vec::new(),
            target_pending: None,
            project: None,
            project_path: None,
            pipeline: None,
            cache: DecodedCache::new(CACHE_CAPACITY, CACHE_BYTE_BUDGET),
            current_index: 0,
            history: Vec::new(),
            saver: None,
            copy_cancel: None,
            prerender_cancel: None,
        }
    }
}

impl AppState {
    pub fn reset_for_new_project(&mut self, project: Project, project_path: PathBuf) {
        self.saver = Some(ProjectSaver::spawn(project_path.clone()));
        self.project = Some(project);
        self.project_path = Some(project_path);
        self.pipeline = Some(DecodePipeline::new(
            marshmallow_core::decode::default_worker_count(),
            DECODE_TARGET_LONG_EDGE,
        ));
        self.cache = DecodedCache::new(CACHE_CAPACITY, CACHE_BYTE_BUDGET);
        self.current_index = 0;
        self.history.clear();
        self.prerender_cancel = None;
    }

    pub fn autosave(&self) {
        if let (Some(project), Some(saver)) = (&self.project, &self.saver) {
            saver.request_save(project.clone());
        }
    }
}

pub struct ProjectSaver {
    tx: std::sync::mpsc::SyncSender<Project>,
}

impl ProjectSaver {
    pub fn spawn(path: PathBuf) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Project>(1);
        std::thread::spawn(move || {
            while let Ok(mut project) = rx.recv() {
                if let Err(e) = project.save(&path) {
                    eprintln!("marshmallow: failed to save project file: {e}");
                }
            }
        });
        Self { tx }
    }

    pub fn request_save(&self, project: Project) {
        let _ = self.tx.try_send(project);
    }
}

pub fn bridge_to_async<T: Send + 'static>(
    rx: crossbeam_channel::Receiver<T>,
) -> async_channel::Receiver<T> {
    let (tx, async_rx) = async_channel::unbounded::<T>();
    std::thread::spawn(move || {
        while let Ok(item) = rx.recv() {
            if tx.send_blocking(item).is_err() {
                break;
            }
        }
    });
    async_rx
}
