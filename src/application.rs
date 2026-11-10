// SPDX-License-Identifier: GPL-3.0-or-later
//! Application setup: actions, keyboard shortcuts and the main window.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::preferences::PreferencesDialog;
use crate::window::Window;
use crate::APP_ID;

const HELP_URL: &str = "https://www.schaffter.ca/mom/";

const ACCELS: &[(&str, &[&str])] = &[
    ("win.open", &["<Control>o"]),
    ("win.save", &["<Control>s"]),
    ("win.save-as", &["<Control><Shift>s"]),
    ("win.compile", &["F5"]),
    ("win.show-log", &["<Control><Shift>l"]),
    (
        "win.zoom-in",
        &["<Control>plus", "<Control>equal", "<Control>KP_Add"],
    ),
    ("win.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]),
    ("win.zoom-fit", &["<Control>0", "<Control>KP_0"]),
    ("win.show-help-overlay", &["<Control>question"]),
    ("app.preferences", &["<Control>comma"]),
    ("app.quit", &["<Control>q"]),
];

pub fn new() -> adw::Application {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .resource_base_path("/dev/calmecac/Myne")
        .build();

    app.connect_startup(|app| {
        gtk::Window::set_default_icon_name(APP_ID);
        setup_actions(app);
        for (action, accels) in ACCELS {
            app.set_accels_for_action(action, accels);
        }
    });
    app.connect_activate(|app| main_window(app).present());
    app.connect_open(|app, files, _hint| {
        let window = main_window(app);
        window.present();
        if let Some(file) = files.first().cloned() {
            glib::spawn_future_local(async move { window.open_file(file).await });
        }
    });
    app
}

/// Myne uses a single window.
fn main_window(app: &adw::Application) -> Window {
    app.windows()
        .into_iter()
        .find_map(|w| w.downcast::<Window>().ok())
        .unwrap_or_else(|| Window::new(app))
}

fn active_window(app: &adw::Application) -> Option<Window> {
    app.active_window().and_downcast::<Window>()
}

fn setup_actions(app: &adw::Application) {
    let preferences = gio::ActionEntry::builder("preferences")
        .activate(|app: &adw::Application, _, _| {
            if let Some(window) = active_window(app) {
                PreferencesDialog::new(&window).present(Some(&window));
            }
        })
        .build();
    let help = gio::ActionEntry::builder("help")
        .activate(|app: &adw::Application, _, _| {
            let window = active_window(app);
            gtk::UriLauncher::new(HELP_URL).launch(window.as_ref(), gio::Cancellable::NONE, |_| {});
        })
        .build();
    let about = gio::ActionEntry::builder("about")
        .activate(|app: &adw::Application, _, _| show_about(active_window(app).as_ref()))
        .build();
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| {
            // Closing (not quitting) goes through the unsaved-changes check.
            for window in app.windows() {
                window.close();
            }
        })
        .build();
    app.add_action_entries([preferences, help, about, quit]);
}

fn show_about(parent: Option<&Window>) {
    let about = adw::AboutDialog::builder()
        .application_name("Myne")
        .application_icon(APP_ID)
        .developer_name("Calmécac.dev")
        .version(env!("CARGO_PKG_VERSION"))
        .website("https://github.com/calmecac-dev/myne")
        .issue_url("https://github.com/calmecac-dev/myne/issues")
        .copyright("© 2026 Calmécac.dev")
        .developers(["Mauricio Cerón Medina"])
        .comments(gettext(
            "An editor for groff documents written with the mom macros, with a PDF preview on save.",
        ))
        // TRANSLATORS: Put your name here, as it should be credited.
        .translator_credits(gettext("translator-credits"))
        .license_type(gtk::License::Gpl30)
        .build();
    about.present(parent);
}
