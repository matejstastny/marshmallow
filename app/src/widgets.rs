use adw::prelude::*;

/// Every GTK widget the app needs to reach again after construction.
/// GTK objects are cheap to clone (refcounted), so this whole struct is
/// `Clone` and closures just capture the handles they need.
#[derive(Clone)]
pub struct Widgets {
    pub window: adw::ApplicationWindow,
    pub stack: gtk4::Stack,
    pub header_title: gtk4::Label,
    pub back_button: gtk4::Button,
    pub prerender_button: gtk4::Button,

    // Setup screen
    pub sources_list: gtk4::ListBox,
    pub target_row: adw::ActionRow,
    pub setup_status_label: gtk4::Label,
    pub add_source_button: gtk4::Button,
    pub choose_target_button: gtk4::Button,
    pub primary_action_button: gtk4::Button,

    // Review screen
    pub media_stack: gtk4::Stack,
    pub picture: gtk4::Picture,
    pub video: gtk4::Video,
    pub review_progress_label: gtk4::Label,
    pub review_filename_label: gtk4::Label,
    pub review_decision_badge: gtk4::Label,
    pub review_ahead_label: gtk4::Label,
    pub key_controller: gtk4::EventControllerKey,

    // Summary screen
    pub summary_counts_label: gtk4::Label,
    pub summary_size_label: gtk4::Label,
    pub summary_banner: adw::Banner,
    pub start_copy_button: gtk4::Button,

    // Copy screen
    pub copy_progress_bar: gtk4::ProgressBar,
    pub copy_current_file_label: gtk4::Label,
    pub copy_cancel_button: gtk4::Button,
    pub copy_done_box: gtk4::Box,
    pub copy_done_label: gtk4::Label,
    pub copy_open_target_button: gtk4::Button,
    pub copy_back_button: gtk4::Button,

    // Pre-render screen
    pub prerender_progress_bar: gtk4::ProgressBar,
    pub prerender_status_label: gtk4::Label,
    pub prerender_cancel_button: gtk4::Button,
    pub prerender_done_box: gtk4::Box,
    pub prerender_done_label: gtk4::Label,
    pub prerender_continue_button: gtk4::Button,
}

pub fn build(app: &adw::Application) -> Widgets {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("marshmallow")
        .default_width(1400)
        .default_height(900)
        .build();

    let header_title = gtk4::Label::new(Some("marshmallow"));
    header_title.add_css_class("title");

    let back_button = gtk4::Button::from_icon_name("go-previous-symbolic");
    back_button.set_tooltip_text(Some("Back to setup (Esc)"));
    back_button.set_visible(false);

    let prerender_button = gtk4::Button::with_label("Pre-render All");
    prerender_button.set_tooltip_text(Some(
        "Decode every photo once and cache a small copy on disk, so browsing later — even outside the look-ahead window — has zero load time. Pauses normal review and uses most CPU cores; best used right before you step away.",
    ));
    prerender_button.set_visible(false);

    let header = adw::HeaderBar::builder()
        .title_widget(&header_title)
        .build();
    header.pack_start(&back_button);
    header.pack_end(&prerender_button);

    let stack = gtk4::Stack::builder()
        .vexpand(true)
        .hexpand(true)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .build();

    let (
        setup_root,
        sources_list,
        target_row,
        setup_status_label,
        add_source_button,
        choose_target_button,
        primary_action_button,
    ) = build_setup_view();
    stack.add_named(&setup_root, Some("setup"));

    let (
        review_root,
        media_stack,
        picture,
        video,
        review_progress_label,
        review_filename_label,
        review_decision_badge,
        review_ahead_label,
        key_controller,
    ) = build_review_view();
    stack.add_named(&review_root, Some("review"));

    let (summary_root, summary_counts_label, summary_size_label, summary_banner, start_copy_button) =
        build_summary_view();
    stack.add_named(&summary_root, Some("summary"));

    let (
        copy_root,
        copy_progress_bar,
        copy_current_file_label,
        copy_cancel_button,
        copy_done_box,
        copy_done_label,
        copy_open_target_button,
        copy_back_button,
    ) = build_copy_view();
    stack.add_named(&copy_root, Some("copy"));

    let (
        prerender_root,
        prerender_progress_bar,
        prerender_status_label,
        prerender_cancel_button,
        prerender_done_box,
        prerender_done_label,
        prerender_continue_button,
    ) = build_prerender_view();
    stack.add_named(&prerender_root, Some("prerender"));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&stack);
    window.set_content(Some(&content));

    Widgets {
        window,
        stack,
        header_title,
        back_button,
        prerender_button,
        sources_list,
        target_row,
        setup_status_label,
        add_source_button,
        choose_target_button,
        primary_action_button,
        media_stack,
        picture,
        video,
        review_progress_label,
        review_filename_label,
        review_decision_badge,
        review_ahead_label,
        key_controller,
        summary_counts_label,
        summary_size_label,
        summary_banner,
        start_copy_button,
        copy_progress_bar,
        copy_current_file_label,
        copy_cancel_button,
        copy_done_box,
        copy_done_label,
        copy_open_target_button,
        copy_back_button,
        prerender_progress_bar,
        prerender_status_label,
        prerender_cancel_button,
        prerender_done_box,
        prerender_done_label,
        prerender_continue_button,
    }
}

#[allow(clippy::type_complexity)]
fn build_setup_view() -> (
    gtk4::Widget,
    gtk4::ListBox,
    adw::ActionRow,
    gtk4::Label,
    gtk4::Button,
    gtk4::Button,
    gtk4::Button,
) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    root.set_margin_top(36);
    root.set_margin_bottom(36);
    root.set_margin_start(48);
    root.set_margin_end(48);
    root.set_valign(gtk4::Align::Center);

    let clamp = adw::Clamp::builder().maximum_size(640).child(&root).build();

    let sources_group = adw::PreferencesGroup::builder()
        .title("Sources")
        .description("Directories or mounted volumes to scan for photos and videos")
        .build();
    let sources_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    sources_group.add(&sources_list);
    let add_source_button = gtk4::Button::with_label("Add Source\u{2026}");
    add_source_button.set_halign(gtk4::Align::Start);
    sources_group.set_header_suffix(Some(&add_source_button));

    let target_group = adw::PreferencesGroup::builder()
        .title("Target")
        .description("Where kept files will be copied to")
        .build();
    let target_row = adw::ActionRow::builder()
        .title("No target selected")
        .build();
    let choose_target_button = gtk4::Button::with_label("Choose\u{2026}");
    choose_target_button.set_valign(gtk4::Align::Center);
    target_row.add_suffix(&choose_target_button);
    target_group.add(&target_row);

    let setup_status_label = gtk4::Label::new(None);
    setup_status_label.set_wrap(true);
    setup_status_label.add_css_class("dim-label");

    let primary_action_button = gtk4::Button::with_label("Start Review");
    primary_action_button.add_css_class("suggested-action");
    primary_action_button.add_css_class("pill");
    primary_action_button.set_sensitive(false);
    primary_action_button.set_halign(gtk4::Align::Center);

    root.append(&sources_group);
    root.append(&target_group);
    root.append(&setup_status_label);
    root.append(&primary_action_button);

    (
        clamp.upcast(),
        sources_list,
        target_row,
        setup_status_label,
        add_source_button,
        choose_target_button,
        primary_action_button,
    )
}

#[allow(clippy::type_complexity)]
fn build_review_view() -> (
    gtk4::Box,
    gtk4::Stack,
    gtk4::Picture,
    gtk4::Video,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::EventControllerKey,
) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.set_hexpand(true);

    let media_stack = gtk4::Stack::builder()
        .vexpand(true)
        .hexpand(true)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(80)
        .build();

    let picture = gtk4::Picture::new();
    picture.set_content_fit(gtk4::ContentFit::Contain);
    picture.set_can_shrink(true);
    media_stack.add_named(&picture, Some("photo"));

    let video = gtk4::Video::builder().autoplay(true).loop_(false).build();
    media_stack.add_named(&video, Some("video"));

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .width_request(48)
        .height_request(48)
        .build();
    media_stack.add_named(&spinner, Some("loading"));

    let empty_label = gtk4::Label::new(Some("No media found in the selected sources"));
    media_stack.add_named(&empty_label, Some("empty"));

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&media_stack));

    let hud = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    hud.set_valign(gtk4::Align::End);
    hud.set_halign(gtk4::Align::Fill);
    hud.add_css_class("osd");
    hud.set_margin_start(12);
    hud.set_margin_end(12);
    hud.set_margin_bottom(12);

    let review_filename_label = gtk4::Label::new(None);
    review_filename_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    review_filename_label.set_hexpand(true);
    review_filename_label.set_halign(gtk4::Align::Start);

    let review_decision_badge = gtk4::Label::new(None);
    review_decision_badge.add_css_class("caption-heading");

    let review_progress_label = gtk4::Label::new(None);
    review_progress_label.add_css_class("dim-label");

    let review_ahead_label = gtk4::Label::new(None);
    review_ahead_label.add_css_class("dim-label");
    review_ahead_label.set_tooltip_text(Some(
        "How many photos ahead of this one are already decoded and ready to render instantly",
    ));

    hud.append(&review_filename_label);
    hud.append(&review_decision_badge);
    hud.append(&review_ahead_label);
    hud.append(&review_progress_label);
    overlay.add_overlay(&hud);

    root.append(&overlay);

    let key_controller = gtk4::EventControllerKey::new();

    (
        root,
        media_stack,
        picture,
        video,
        review_progress_label,
        review_filename_label,
        review_decision_badge,
        review_ahead_label,
        key_controller,
    )
}

#[allow(clippy::type_complexity)]
fn build_summary_view() -> (
    gtk4::Widget,
    gtk4::Label,
    gtk4::Label,
    adw::Banner,
    gtk4::Button,
) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    root.set_valign(gtk4::Align::Center);
    root.set_margin_top(36);
    root.set_margin_bottom(36);
    root.set_margin_start(48);
    root.set_margin_end(48);

    let clamp = adw::Clamp::builder().maximum_size(560).child(&root).build();

    let title = gtk4::Label::new(Some("Review Summary"));
    title.add_css_class("title-1");

    let summary_counts_label = gtk4::Label::new(None);
    summary_counts_label.set_wrap(true);
    summary_counts_label.set_justify(gtk4::Justification::Center);

    let summary_size_label = gtk4::Label::new(None);
    summary_size_label.add_css_class("dim-label");

    let summary_banner = adw::Banner::new("");
    summary_banner.set_revealed(false);

    let start_copy_button = gtk4::Button::with_label("Start Copy");
    start_copy_button.add_css_class("suggested-action");
    start_copy_button.add_css_class("pill");
    start_copy_button.set_halign(gtk4::Align::Center);

    root.append(&title);
    root.append(&summary_counts_label);
    root.append(&summary_size_label);
    root.append(&summary_banner);
    root.append(&start_copy_button);

    (
        clamp.upcast(),
        summary_counts_label,
        summary_size_label,
        summary_banner,
        start_copy_button,
    )
}

#[allow(clippy::type_complexity)]
fn build_copy_view() -> (
    gtk4::Widget,
    gtk4::ProgressBar,
    gtk4::Label,
    gtk4::Button,
    gtk4::Box,
    gtk4::Label,
    gtk4::Button,
    gtk4::Button,
) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    root.set_valign(gtk4::Align::Center);
    root.set_margin_top(36);
    root.set_margin_bottom(36);
    root.set_margin_start(48);
    root.set_margin_end(48);

    let clamp = adw::Clamp::builder().maximum_size(560).child(&root).build();

    let copy_progress_bar = gtk4::ProgressBar::builder().show_text(true).build();
    let copy_current_file_label = gtk4::Label::new(Some("Preparing\u{2026}"));
    copy_current_file_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    copy_current_file_label.add_css_class("dim-label");

    let copy_cancel_button = gtk4::Button::with_label("Cancel");
    copy_cancel_button.add_css_class("destructive-action");
    copy_cancel_button.set_halign(gtk4::Align::Center);

    let copy_done_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    copy_done_box.set_visible(false);
    let copy_done_label = gtk4::Label::new(None);
    copy_done_label.set_wrap(true);
    copy_done_label.set_justify(gtk4::Justification::Center);
    let copy_open_target_button = gtk4::Button::with_label("Open Target Folder");
    copy_open_target_button.set_halign(gtk4::Align::Center);
    let copy_back_button = gtk4::Button::with_label("Back to Setup");
    copy_back_button.set_halign(gtk4::Align::Center);
    copy_done_box.append(&copy_done_label);
    copy_done_box.append(&copy_open_target_button);
    copy_done_box.append(&copy_back_button);

    root.append(&copy_progress_bar);
    root.append(&copy_current_file_label);
    root.append(&copy_cancel_button);
    root.append(&copy_done_box);

    (
        clamp.upcast(),
        copy_progress_bar,
        copy_current_file_label,
        copy_cancel_button,
        copy_done_box,
        copy_done_label,
        copy_open_target_button,
        copy_back_button,
    )
}

#[allow(clippy::type_complexity)]
fn build_prerender_view() -> (
    gtk4::Widget,
    gtk4::ProgressBar,
    gtk4::Label,
    gtk4::Button,
    gtk4::Box,
    gtk4::Label,
    gtk4::Button,
) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    root.set_valign(gtk4::Align::Center);
    root.set_margin_top(36);
    root.set_margin_bottom(36);
    root.set_margin_start(48);
    root.set_margin_end(48);

    let clamp = adw::Clamp::builder().maximum_size(560).child(&root).build();

    let title = gtk4::Label::new(Some("Pre-rendering"));
    title.add_css_class("title-1");

    let prerender_progress_bar = gtk4::ProgressBar::builder().show_text(true).build();
    let prerender_status_label = gtk4::Label::new(Some("Starting…"));
    prerender_status_label.set_wrap(true);
    prerender_status_label.set_justify(gtk4::Justification::Center);
    prerender_status_label.add_css_class("dim-label");

    let prerender_cancel_button = gtk4::Button::with_label("Cancel");
    prerender_cancel_button.add_css_class("destructive-action");
    prerender_cancel_button.set_halign(gtk4::Align::Center);

    let prerender_done_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    prerender_done_box.set_visible(false);
    let prerender_done_label = gtk4::Label::new(None);
    prerender_done_label.set_wrap(true);
    prerender_done_label.set_justify(gtk4::Justification::Center);
    let prerender_continue_button = gtk4::Button::with_label("Continue Review");
    prerender_continue_button.add_css_class("suggested-action");
    prerender_continue_button.add_css_class("pill");
    prerender_continue_button.set_halign(gtk4::Align::Center);
    prerender_done_box.append(&prerender_done_label);
    prerender_done_box.append(&prerender_continue_button);

    root.append(&title);
    root.append(&prerender_progress_bar);
    root.append(&prerender_status_label);
    root.append(&prerender_cancel_button);
    root.append(&prerender_done_box);

    (
        clamp.upcast(),
        prerender_progress_bar,
        prerender_status_label,
        prerender_cancel_button,
        prerender_done_box,
        prerender_done_label,
        prerender_continue_button,
    )
}
