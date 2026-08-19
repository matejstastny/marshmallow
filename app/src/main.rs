mod actions;
mod keybinds;
mod state;
mod widgets;

use adw::prelude::*;

const APP_ID: &str = "dev.matysta.Marshmallow";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let widgets = widgets::build(app);
    let state = actions::new_state();

    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.add_source_button.clone();
        btn.connect_clicked(move |_| actions::add_source(&state, &widgets));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.choose_target_button.clone();
        btn.connect_clicked(move |_| actions::choose_target(&state, &widgets));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.primary_action_button.clone();
        btn.connect_clicked(move |_| actions::primary_action(&state, &widgets));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.back_button.clone();
        btn.connect_clicked(move |_| actions::go_to_setup(&state, &widgets));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.prerender_button.clone();
        btn.connect_clicked(move |_| actions::start_prerender(&state, &widgets));
    }

    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.start_copy_button.clone();
        btn.connect_clicked(move |_| actions::start_copy(&state, &widgets));
    }

    {
        let state = state.clone();
        let btn = widgets.copy_cancel_button.clone();
        btn.connect_clicked(move |_| actions::cancel_copy(&state));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.copy_open_target_button.clone();
        btn.connect_clicked(move |_| actions::open_target_folder(&state, &widgets));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.copy_back_button.clone();
        btn.connect_clicked(move |_| actions::reset_to_fresh_setup(&state, &widgets));
    }

    {
        let state = state.clone();
        let btn = widgets.prerender_cancel_button.clone();
        btn.connect_clicked(move |_| actions::cancel_prerender(&state));
    }
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let btn = widgets.prerender_continue_button.clone();
        btn.connect_clicked(move |_| actions::continue_review(&state, &widgets));
    }

    {
        let state = state.clone();
        let widgets = widgets.clone();
        let controller = widgets.key_controller.clone();
        controller.connect_key_pressed(move |_controller, key, _keycode, _modifiers| {
            if widgets.stack.visible_child_name().as_deref() != Some("review") {
                return glib::Propagation::Proceed;
            }
            match keybinds::action_for_key(key) {
                Some(action) => {
                    actions::handle_review_action(&state, &widgets, action);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
    }
    widgets
        .window
        .add_controller(widgets.key_controller.clone());

    actions::try_restore_recent(&state, &widgets);
    actions::update_setup_state(&state, &widgets);
    widgets.window.present();
}
