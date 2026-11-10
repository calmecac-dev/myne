// SPDX-License-Identifier: GPL-3.0-or-later
//! Helpers for the source editor (`GtkSourceView`).

use adw::prelude::*;
use gtk::glib;
use sourceview5::prelude::*;

/// Replaces the buffer contents without creating an undo step, and marks
/// the buffer as unmodified.
pub fn set_text(buffer: &sourceview5::Buffer, text: &str) {
    buffer.begin_irreversible_action();
    buffer.set_text(text);
    buffer.end_irreversible_action();
    buffer.set_modified(false);
    buffer.place_cursor(&buffer.start_iter());
}

pub fn text(buffer: &sourceview5::Buffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

/// Text for the position indicator: "Línea 3, col. 14".
pub fn cursor_position_text(buffer: &sourceview5::Buffer) -> String {
    let iter = buffer.iter_at_offset(buffer.cursor_position());
    format!("Línea {}, col. {}", iter.line() + 1, iter.line_offset() + 1)
}

/// Keeps the buffer's style scheme in step with the system light/dark setting.
pub fn follow_system_theme(buffer: &sourceview5::Buffer) {
    let apply = glib::clone!(
        #[weak]
        buffer,
        move |manager: &adw::StyleManager| {
            let name = if manager.is_dark() {
                "Adwaita-dark"
            } else {
                "Adwaita"
            };
            buffer.set_style_scheme(
                sourceview5::StyleSchemeManager::default()
                    .scheme(name)
                    .as_ref(),
            );
        }
    );
    let manager = adw::StyleManager::default();
    apply(&manager);
    manager.connect_dark_notify(apply);
}
