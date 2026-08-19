use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adw::prelude::*;
use gtk4::glib;

use marshmallow_core::copy::{CopyEngine, CopyOutcome, CopyProgress};
use marshmallow_core::decode::{window_around, DecodeResult, DecodedCache, DecodedImage};
use marshmallow_core::media::MediaKind;
use marshmallow_core::project::{Decision, Project, Source};
use marshmallow_core::scan;

use crate::keybinds::ReviewAction;
use crate::state::{
    bridge_to_async, AppState, CACHE_BYTE_BUDGET, CACHE_CAPACITY, WINDOW_AHEAD, WINDOW_BEHIND,
};
use crate::widgets::Widgets;

pub type SharedState = Rc<RefCell<AppState>>;

pub fn new_state() -> SharedState {
    Rc::new(RefCell::new(AppState::default()))
}

fn switch_screen(widgets: &Widgets, name: &str) {
    widgets.stack.set_visible_child_name(name);
    widgets.header_title.set_text(match name {
        "setup" => "marshmallow",
        "review" => "Review",
        "summary" => "Review Summary",
        "copy" => "Copying",
        "prerender" => "Pre-rendering",
        _ => "marshmallow",
    });
    widgets.back_button.set_visible(name != "setup");
    widgets
        .prerender_button
        .set_visible(matches!(name, "review" | "summary"));
}

fn effective_decode_path(
    target: &std::path::Path,
    sources: &[Source],
    item: &marshmallow_core::project::MediaItem,
) -> Option<std::path::PathBuf> {
    let cache_path =
        marshmallow_core::prerender::cache_path_for(target, item.source_id, &item.relative_path);
    if cache_path.exists() {
        Some(cache_path)
    } else {
        item.absolute_path(sources)
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn build_texture(image: &DecodedImage) -> gdk4::Texture {
    let stride = image.width as usize * 4;
    let bytes = glib::Bytes::from_owned(image.rgba.clone());
    gdk4::MemoryTexture::new(
        image.width as i32,
        image.height as i32,
        gdk4::MemoryFormat::R8g8b8a8,
        &bytes,
        stride,
    )
    .upcast()
}

pub fn add_source(state: &SharedState, widgets: &Widgets) {
    let dialog = gtk4::FileDialog::builder()
        .title("Select a Source Directory")
        .build();
    let state = state.clone();
    let widgets = widgets.clone();
    let window = widgets.window.clone();
    glib::spawn_future_local(async move {
        if let Ok(folder) = dialog.select_folder_future(Some(&window)).await {
            if let Some(path) = folder.path() {
                {
                    let mut st = state.borrow_mut();
                    let id = scan::next_source_id(&st.sources_pending);
                    st.sources_pending.push(Source { id, path });
                }
                rebuild_sources_list(&state, &widgets);
                update_setup_state(&state, &widgets);
            }
        }
    });
}

pub fn choose_target(state: &SharedState, widgets: &Widgets) {
    let dialog = gtk4::FileDialog::builder()
        .title("Select Target Directory")
        .build();
    let state = state.clone();
    let widgets = widgets.clone();
    let window = widgets.window.clone();
    glib::spawn_future_local(async move {
        if let Ok(folder) = dialog.select_folder_future(Some(&window)).await {
            if let Some(path) = folder.path() {
                state.borrow_mut().target_pending = Some(path);
                update_setup_state(&state, &widgets);
            }
        }
    });
}

pub fn rebuild_sources_list(state: &SharedState, widgets: &Widgets) {
    while let Some(child) = widgets.sources_list.first_child() {
        widgets.sources_list.remove(&child);
    }
    let sources = state.borrow().sources_pending.clone();
    for (i, source) in sources.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(source.path.display().to_string())
            .build();
        let remove_btn = gtk4::Button::from_icon_name("list-remove-symbolic");
        remove_btn.add_css_class("flat");
        remove_btn.set_valign(gtk4::Align::Center);
        let state_clone = state.clone();
        let widgets_clone = widgets.clone();
        remove_btn.connect_clicked(move |_| {
            {
                let mut st = state_clone.borrow_mut();
                if i < st.sources_pending.len() {
                    st.sources_pending.remove(i);
                }
            }
            rebuild_sources_list(&state_clone, &widgets_clone);
            update_setup_state(&state_clone, &widgets_clone);
        });
        row.add_suffix(&remove_btn);
        widgets.sources_list.append(&row);
    }
}

pub fn update_setup_state(state: &SharedState, widgets: &Widgets) {
    let (has_active_project, target, has_sources) = {
        let st = state.borrow();
        (
            st.project.is_some(),
            st.target_pending.clone(),
            !st.sources_pending.is_empty(),
        )
    };

    if let Some(target) = &target {
        widgets.target_row.set_title("Target");
        widgets
            .target_row
            .set_subtitle(&target.display().to_string());
    } else {
        widgets.target_row.set_title("No target selected");
        widgets.target_row.set_subtitle("");
    }

    if has_active_project {
        widgets.primary_action_button.set_label("Resume Review");
        widgets.primary_action_button.set_sensitive(true);
        widgets
            .setup_status_label
            .set_text("A review is already in progress in this session.");
        return;
    }

    let resumable = target
        .as_ref()
        .map(|t| Project::default_path(t).exists())
        .unwrap_or(false);

    if resumable {
        widgets.primary_action_button.set_label("Resume Review");
        widgets.primary_action_button.set_sensitive(true);
        widgets
            .setup_status_label
            .set_text("An existing project was found at this target — resuming will load its saved keep/trash decisions.");
    } else {
        widgets.primary_action_button.set_label("Start Review");
        widgets
            .primary_action_button
            .set_sensitive(has_sources && target.is_some());
        widgets.setup_status_label.set_text(if target.is_none() {
            "Add at least one source and choose a target to begin."
        } else if !has_sources {
            "Add at least one source directory."
        } else {
            ""
        });
    }
}

pub fn try_restore_recent(state: &SharedState, widgets: &Widgets) {
    let Some(recent) = marshmallow_core::recent::RecentProject::load() else {
        return;
    };
    let default_path = Project::default_path(&recent.target);
    let Ok(project) = Project::load(&default_path) else {
        return;
    };

    {
        let mut st = state.borrow_mut();
        st.target_pending = Some(recent.target);
        st.sources_pending = project.sources;
    }
    rebuild_sources_list(state, widgets);
    update_setup_state(state, widgets);
}

pub fn primary_action(state: &SharedState, widgets: &Widgets) {
    let already_active = state.borrow().project.is_some();
    if already_active {
        render_current(state, widgets);
        switch_screen(widgets, "review");
        return;
    }

    let target = state.borrow().target_pending.clone();
    let Some(target) = target else { return };
    let default_path = Project::default_path(&target);

    let project = if default_path.exists() {
        match Project::load(&default_path) {
            Ok(p) => p,
            Err(e) => {
                widgets
                    .setup_status_label
                    .set_text(&format!("Failed to load existing project: {e}"));
                return;
            }
        }
    } else {
        let sources = state.borrow().sources_pending.clone();
        if sources.is_empty() {
            return;
        }
        let items = scan::scan_sources(&sources);
        let mut project = Project::new(sources, target.clone(), items);
        if let Err(e) = project.save(&default_path) {
            widgets
                .setup_status_label
                .set_text(&format!("Failed to create project file: {e}"));
            return;
        }
        project
    };

    let _ = marshmallow_core::recent::RecentProject::save(&target);
    state
        .borrow_mut()
        .reset_for_new_project(project, default_path);
    spawn_decode_bridge(state, widgets);
    jump_to_first_undecided(state);
    request_prefetch_window(state);
    render_current(state, widgets);
    switch_screen(widgets, "review");
}

pub fn go_to_setup(state: &SharedState, widgets: &Widgets) {
    switch_screen(widgets, "setup");
    update_setup_state(state, widgets);
}

pub fn reset_to_fresh_setup(state: &SharedState, widgets: &Widgets) {
    *state.borrow_mut() = AppState::default();
    rebuild_sources_list(state, widgets);
    update_setup_state(state, widgets);
    switch_screen(widgets, "setup");
}

fn spawn_decode_bridge(state: &SharedState, widgets: &Widgets) {
    let result_rx = state
        .borrow()
        .pipeline
        .as_ref()
        .map(|p| p.result_rx.clone());
    let Some(result_rx) = result_rx else { return };
    let async_rx = bridge_to_async(result_rx);
    let state = state.clone();
    let widgets = widgets.clone();
    glib::spawn_future_local(async move {
        while let Ok(result) = async_rx.recv().await {
            on_decode_result(&state, &widgets, result);
        }
    });
}

fn jump_to_first_undecided(state: &SharedState) {
    let mut st = state.borrow_mut();
    let Some(project) = &st.project else { return };
    st.current_index = project
        .items
        .iter()
        .position(|i| i.decision == Decision::Undecided)
        .unwrap_or(0);
}

pub fn handle_review_action(state: &SharedState, widgets: &Widgets, action: ReviewAction) {
    match action {
        ReviewAction::Keep => decide(state, widgets, Decision::Keep),
        ReviewAction::Trash => decide(state, widgets, Decision::Trash),
        ReviewAction::Previous => move_relative(state, widgets, -1),
        ReviewAction::Next => move_relative(state, widgets, 1),
        ReviewAction::Undo => undo(state, widgets),
        ReviewAction::JumpToNextUndecided => jump_to_next_undecided(state, widgets),
        ReviewAction::PlayPause => toggle_play_pause(widgets),
        ReviewAction::ToggleFullscreen => toggle_fullscreen(widgets),
        ReviewAction::BackToSetup => go_to_setup(state, widgets),
    }
}

fn decide(state: &SharedState, widgets: &Widgets, decision: Decision) {
    let outcome = {
        let mut st = state.borrow_mut();
        let idx = st.current_index;
        let len = match &st.project {
            Some(p) => p.items.len(),
            None => return,
        };
        if idx >= len {
            return;
        }
        let old = st.project.as_ref().unwrap().items[idx].decision;
        {
            let project = st.project.as_mut().unwrap();
            project.items[idx].decision = decision;
            project.items[idx].decided_at = Some(chrono::Utc::now());
        }
        st.history.push((idx, old));
        st.autosave();
        if idx + 1 < len {
            Some(idx + 1)
        } else {
            None
        }
    };

    match outcome {
        Some(next) => set_current_index(state, widgets, next, true),
        None => show_summary(state, widgets),
    }
}

fn undo(state: &SharedState, widgets: &Widgets) {
    let jump_to = {
        let mut st = state.borrow_mut();
        let Some((idx, old_decision)) = st.history.pop() else {
            return;
        };
        if let Some(project) = st.project.as_mut() {
            if idx < project.items.len() {
                project.items[idx].decision = old_decision;
                project.items[idx].decided_at = None;
            }
        }
        st.autosave();
        idx
    };
    set_current_index(state, widgets, jump_to, false);
}

fn move_relative(state: &SharedState, widgets: &Widgets, delta: i64) {
    let new_index = {
        let st = state.borrow();
        let Some(project) = &st.project else { return };
        let len = project.items.len();
        if len == 0 {
            return;
        }
        (st.current_index as i64 + delta).clamp(0, len as i64 - 1) as usize
    };
    set_current_index(state, widgets, new_index, true);
}

fn jump_to_next_undecided(state: &SharedState, widgets: &Widgets) {
    let target = {
        let st = state.borrow();
        let Some(project) = &st.project else { return };
        let len = project.items.len();
        if len == 0 {
            return;
        }
        let start = st.current_index;
        (1..=len)
            .map(|offset| (start + offset) % len)
            .find(|&i| project.items[i].decision == Decision::Undecided)
    };
    match target {
        Some(idx) => set_current_index(state, widgets, idx, false),
        None => show_summary(state, widgets),
    }
}

fn set_current_index(state: &SharedState, widgets: &Widgets, index: usize, contiguous: bool) {
    {
        let mut st = state.borrow_mut();
        st.current_index = index;
        if !contiguous {
            if let Some(pipeline) = &st.pipeline {
                pipeline.bump_generation();
            }
        }
    }
    request_prefetch_window(state);
    render_current(state, widgets);
}

fn request_prefetch_window(state: &SharedState) {
    let mut st = state.borrow_mut();
    let Some(project) = st.project.as_ref() else {
        return;
    };
    let len = project.items.len();
    if len == 0 {
        return;
    }
    let current = st.current_index;
    let range = window_around(current, len, WINDOW_BEHIND, WINDOW_AHEAD);
    let wide_start = range.start.saturating_sub(2);
    let wide_end = (range.end + 4).min(len);

    let mut to_request = Vec::new();
    for idx in range.clone() {
        let item = &project.items[idx];
        if item.kind == MediaKind::Photo && !st.cache.contains(idx) {
            if let Some(path) = effective_decode_path(&project.target, &project.sources, item) {
                to_request.push((idx, path));
            }
        }
    }

    st.cache.evict_outside(wide_start..wide_end, current);

    if let Some(pipeline) = &st.pipeline {
        for (idx, path) in to_request {
            pipeline.request_prefetch(idx, path);
        }
    }
}

fn on_decode_result(state: &SharedState, widgets: &Widgets, result: DecodeResult) {
    let should_render = {
        let mut st = state.borrow_mut();
        let current_gen = match &st.pipeline {
            Some(p) => p.generation(),
            None => return,
        };
        if result.generation != current_gen {
            return;
        }
        match result.outcome {
            Ok(image) => st.cache.insert(result.index, image),
            Err(e) => {
                eprintln!("marshmallow: decode failed: {e}");
                return;
            }
        }
        let should_render = result.index == st.current_index;
        if !should_render {
            if let Some(project) = st.project.as_ref() {
                let text = ahead_label_text(project, &st.cache, st.current_index);
                widgets.review_ahead_label.set_text(&text);
            }
        }
        should_render
    };
    if should_render {
        render_current(state, widgets);
    }
}

fn ahead_ready_count(
    project: &Project,
    cache: &marshmallow_core::decode::DecodedCache,
    current: usize,
) -> usize {
    let mut count = 0;
    for idx in (current + 1)..project.items.len() {
        match project.items[idx].kind {
            MediaKind::Photo if cache.contains(idx) => count += 1,
            _ => break,
        }
    }
    count
}

fn ahead_label_text(
    project: &Project,
    cache: &marshmallow_core::decode::DecodedCache,
    current: usize,
) -> String {
    format!(
        "{}/{WINDOW_AHEAD} ahead ready",
        ahead_ready_count(project, cache, current)
    )
}

pub fn render_current(state: &SharedState, widgets: &Widgets) {
    let mut st = state.borrow_mut();
    let (item, sources, target, idx, total, keep_count, trash_count) = {
        let Some(project) = st.project.as_ref() else {
            return;
        };
        if project.items.is_empty() {
            widgets.media_stack.set_visible_child_name("empty");
            widgets.review_filename_label.set_text("");
            widgets.review_decision_badge.set_text("");
            widgets.review_progress_label.set_text("0 / 0");
            widgets.review_ahead_label.set_text("");
            return;
        }
        let idx = st.current_index.min(project.items.len() - 1);
        (
            project.items[idx].clone(),
            project.sources.clone(),
            project.target.clone(),
            idx,
            project.items.len(),
            project.keep_count(),
            project.trash_count(),
        )
    };
    st.current_index = idx;

    widgets
        .review_filename_label
        .set_text(&item.relative_path.display().to_string());
    widgets.review_decision_badge.set_text(match item.decision {
        Decision::Undecided => "",
        Decision::Keep => "KEEP",
        Decision::Trash => "TRASH",
    });
    widgets.review_progress_label.set_text(&format!(
        "{} / {} · {} kept · {} trashed",
        idx + 1,
        total,
        keep_count,
        trash_count
    ));

    let abs_path = item.absolute_path(&sources);

    match item.kind {
        MediaKind::Video => {
            widgets.media_stack.set_visible_child_name("video");
            if let Some(path) = &abs_path {
                widgets.video.set_file(Some(&gio::File::for_path(path)));
            }
        }
        MediaKind::Photo => {
            widgets.video.set_file(None::<&gio::File>);
            if let Some(image) = st.cache.get(idx) {
                let texture = build_texture(image);
                widgets.picture.set_paintable(Some(&texture));
                widgets.media_stack.set_visible_child_name("photo");
            } else {
                widgets.media_stack.set_visible_child_name("loading");
                let decode_path = effective_decode_path(&target, &sources, &item);
                if let (Some(pipeline), Some(path)) = (&st.pipeline, &decode_path) {
                    pipeline.request_priority(idx, path.clone());
                }
            }
        }
    }

    if let Some(project) = st.project.as_ref() {
        let text = ahead_label_text(project, &st.cache, idx);
        widgets.review_ahead_label.set_text(&text);
    }
}

fn toggle_play_pause(widgets: &Widgets) {
    if widgets.media_stack.visible_child_name().as_deref() != Some("video") {
        return;
    }
    if let Some(stream) = widgets.video.media_stream() {
        if stream.is_playing() {
            stream.pause();
        } else {
            stream.play();
        }
    }
}

fn toggle_fullscreen(widgets: &Widgets) {
    if widgets.window.is_fullscreen() {
        widgets.window.unfullscreen();
    } else {
        widgets.window.fullscreen();
    }
}

fn show_summary(state: &SharedState, widgets: &Widgets) {
    let (keep_count, trash_count, undecided_count, kept_bytes, target) = {
        let st = state.borrow();
        let Some(project) = &st.project else { return };
        (
            project.keep_count(),
            project.trash_count(),
            project.undecided_count(),
            project.kept_bytes(),
            project.target.clone(),
        )
    };

    widgets.summary_counts_label.set_text(&format!(
        "{keep_count} kept · {trash_count} trashed · {undecided_count} undecided\n(undecided items stay undecided — they won't be copied)"
    ));
    widgets
        .summary_size_label
        .set_text(&format!("{} to copy", format_bytes(kept_bytes)));

    match fs4::available_space(&target) {
        Ok(free) if free < kept_bytes => {
            widgets.summary_banner.set_title(&format!(
                "Only {} free on target — {} needed",
                format_bytes(free),
                format_bytes(kept_bytes)
            ));
            widgets.summary_banner.set_revealed(true);
        }
        _ => widgets.summary_banner.set_revealed(false),
    }

    switch_screen(widgets, "summary");
}

pub fn start_copy(state: &SharedState, widgets: &Widgets) {
    let project = match &state.borrow().project {
        Some(p) => p.clone(),
        None => return,
    };

    widgets.copy_done_box.set_visible(false);
    widgets.copy_cancel_button.set_visible(true);
    widgets.copy_cancel_button.set_sensitive(true);
    widgets.copy_progress_bar.set_fraction(0.0);
    widgets.copy_current_file_label.set_text("Preparing…");
    switch_screen(widgets, "copy");

    let cancel = Arc::new(AtomicBool::new(false));
    state.borrow_mut().copy_cancel = Some(cancel.clone());

    let (progress_tx, progress_rx) = crossbeam_channel::unbounded::<CopyProgress>();
    let progress_rx_async = bridge_to_async(progress_rx);
    let (outcome_tx, outcome_rx) = async_channel::bounded::<anyhow::Result<CopyOutcome>>(1);

    std::thread::spawn(move || {
        let outcome = CopyEngine::run(&project, &progress_tx, &cancel);
        let _ = outcome_tx.send_blocking(outcome);
    });

    {
        let widgets = widgets.clone();
        glib::spawn_future_local(async move {
            while let Ok(p) = progress_rx_async.recv().await {
                update_copy_progress(&widgets, &p);
            }
        });
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        glib::spawn_future_local(async move {
            if let Ok(result) = outcome_rx.recv().await {
                on_copy_finished(&state, &widgets, result);
            }
        });
    }
}

fn update_copy_progress(widgets: &Widgets, p: &CopyProgress) {
    let fraction = if p.bytes_total > 0 {
        p.bytes_done as f64 / p.bytes_total as f64
    } else {
        0.0
    };
    widgets
        .copy_progress_bar
        .set_fraction(fraction.clamp(0.0, 1.0));
    widgets
        .copy_progress_bar
        .set_text(Some(&format!("{}/{} files", p.files_done, p.files_total)));
    if let Some(current) = &p.current_file {
        widgets
            .copy_current_file_label
            .set_text(&current.display().to_string());
    }
}

fn on_copy_finished(state: &SharedState, widgets: &Widgets, result: anyhow::Result<CopyOutcome>) {
    widgets.copy_cancel_button.set_visible(false);
    match result {
        Ok(outcome) => {
            state.borrow_mut().copy_cancel = None;
            let mut text = format!(
                "{} copied · {} skipped · {} renamed · {} failed\nLog: {}",
                outcome.files_copied,
                outcome.files_skipped,
                outcome.files_renamed,
                outcome.files_failed,
                outcome.log_path.display()
            );
            if outcome.cancelled {
                text.push_str(
                    "\n\nCopy was cancelled — files already copied remain on the target.",
                );
            }
            widgets.copy_done_label.set_text(&text);
            widgets.copy_progress_bar.set_fraction(1.0);
        }
        Err(e) => {
            widgets
                .copy_done_label
                .set_text(&format!("Copy failed: {e}"));
        }
    }
    widgets.copy_done_box.set_visible(true);
}

pub fn cancel_copy(state: &SharedState) {
    if let Some(cancel) = &state.borrow().copy_cancel {
        cancel.store(true, Ordering::Relaxed);
    }
}

pub fn open_target_folder(state: &SharedState, widgets: &Widgets) {
    let target = match &state.borrow().project {
        Some(p) => p.target.clone(),
        None => return,
    };
    let launcher = gtk4::FileLauncher::new(Some(&gio::File::for_path(&target)));
    let window = widgets.window.clone();
    glib::spawn_future_local(async move {
        let _ = launcher.launch_future(Some(&window)).await;
    });
}

pub fn start_prerender(state: &SharedState, widgets: &Widgets) {
    if state.borrow().prerender_cancel.is_some() {
        return;
    }
    let project = match &state.borrow().project {
        Some(p) => p.clone(),
        None => return,
    };

    state.borrow_mut().cache = DecodedCache::new(CACHE_CAPACITY, CACHE_BYTE_BUDGET);

    let cancel = Arc::new(AtomicBool::new(false));
    state.borrow_mut().prerender_cancel = Some(cancel.clone());

    widgets.prerender_progress_bar.set_fraction(0.0);
    widgets.prerender_progress_bar.set_text(Some("Starting…"));
    widgets
        .prerender_status_label
        .set_text("Decoding every photo once so later browsing has zero wait, even outside the look-ahead window.");
    widgets.prerender_cancel_button.set_visible(true);
    widgets.prerender_done_box.set_visible(false);
    switch_screen(widgets, "prerender");

    let (progress_tx, progress_rx) =
        crossbeam_channel::unbounded::<marshmallow_core::prerender::PrerenderProgress>();
    let progress_rx_async = bridge_to_async(progress_rx);
    let (outcome_tx, outcome_rx) =
        async_channel::bounded::<anyhow::Result<marshmallow_core::prerender::PrerenderOutcome>>(1);

    let worker_count = marshmallow_core::decode::default_worker_count();
    std::thread::spawn(move || {
        let outcome = marshmallow_core::prerender::PrerenderEngine::run(
            &project,
            crate::state::DECODE_TARGET_LONG_EDGE,
            worker_count,
            &progress_tx,
            &cancel,
        );
        let _ = outcome_tx.send_blocking(outcome);
    });

    {
        let widgets = widgets.clone();
        glib::spawn_future_local(async move {
            while let Ok(p) = progress_rx_async.recv().await {
                let fraction = if p.total > 0 {
                    p.done as f64 / p.total as f64
                } else {
                    1.0
                };
                widgets.prerender_progress_bar.set_fraction(fraction);
                widgets
                    .prerender_progress_bar
                    .set_text(Some(&format!("{}/{} photos", p.done, p.total)));
            }
        });
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        glib::spawn_future_local(async move {
            if let Ok(result) = outcome_rx.recv().await {
                on_prerender_finished(&state, &widgets, result);
            }
        });
    }
}

pub fn cancel_prerender(state: &SharedState) {
    if let Some(cancel) = &state.borrow().prerender_cancel {
        cancel.store(true, Ordering::Relaxed);
    }
}

fn on_prerender_finished(
    state: &SharedState,
    widgets: &Widgets,
    result: anyhow::Result<marshmallow_core::prerender::PrerenderOutcome>,
) {
    state.borrow_mut().prerender_cancel = None;
    widgets.prerender_cancel_button.set_visible(false);
    let text = match result {
        Ok(outcome) if outcome.cancelled => {
            format!(
                "Cancelled — {} new photos rendered before stopping ({} were already cached).",
                outcome.rendered, outcome.skipped
            )
        }
        Ok(outcome) => {
            widgets.prerender_progress_bar.set_fraction(1.0);
            let failed_note = if outcome.failed > 0 {
                format!(", {} failed", outcome.failed)
            } else {
                String::new()
            };
            format!(
                "All photos pre-rendered — {} new, {} already cached{failed_note}.",
                outcome.rendered, outcome.skipped
            )
        }
        Err(e) => format!("Pre-render failed: {e}"),
    };
    widgets.prerender_done_label.set_text(&text);
    widgets.prerender_done_box.set_visible(true);
}

pub fn continue_review(state: &SharedState, widgets: &Widgets) {
    switch_screen(widgets, "review");
    request_prefetch_window(state);
    render_current(state, widgets);
}
