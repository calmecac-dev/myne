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

fn main() -> glib::ExitCode {
    if let Err(error) = gio::resources_register_include!("myne.gresource") {
        eprintln!("myne: no se pudieron cargar los recursos: {error}");
        return glib::ExitCode::FAILURE;
    }
    application::new().run()
}
