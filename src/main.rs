// SPDX-License-Identifier: GPL-3.0-or-later
//! Myne: editor for groff documents written with the mom macros.

mod application;
mod compiler;
mod config;
mod editor;
mod log_panel;
mod preferences;
mod viewer;
mod window;

use gtk::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "dev.calmecac.Myne";

/// Loads the translations for the user's language. They live in the first
/// XDG data directory that has Myne's, `~/.local/share/locale` during
/// development and the system one once installed.
fn init_locale() {
    use gettextrs::{
        bind_textdomain_codeset, bindtextdomain, setlocale, textdomain, LocaleCategory,
    };
    // SAFETY: called first thing in main, before any other thread exists.
    unsafe {
        // gettext, GTK and libadwaita all read LANGUAGE first.
        let language = config::Config::load().0.language;
        if !language.is_empty() {
            std::env::set_var("LANGUAGE", language);
        }
        setlocale(LocaleCategory::LcAll, "");
    }
    let dir = std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .map(|d| d.join("locale"))
        .find(|d| {
            std::fs::read_dir(d).is_ok_and(|langs| {
                langs
                    .flatten()
                    .any(|l| l.path().join("LC_MESSAGES/myne.mo").exists())
            })
        });
    if let Some(dir) = dir {
        let _ = bindtextdomain("myne", dir);
    }
    let _ = bind_textdomain_codeset("myne", "UTF-8");
    let _ = textdomain("myne");
}

fn main() -> glib::ExitCode {
    init_locale();
    if let Err(error) = gio::resources_register_include!("myne.gresource") {
        eprintln!("myne: could not load the resources: {error}");
        return glib::ExitCode::FAILURE;
    }
    application::new().run()
}
