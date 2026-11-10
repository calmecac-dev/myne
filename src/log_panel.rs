// SPDX-License-Identifier: GPL-3.0-or-later
//! Compilation log: one row per message in the `log_list` of the window.

use adw::prelude::*;
use gtk::glib;

use crate::compiler::{Diagnostic, Severity};

pub struct LogPanel {
    list: gtk::ListBox,
}

impl LogPanel {
    pub fn new(list: gtk::ListBox) -> Self {
        // Follow the output, like a terminal: whenever rows are added the
        // scrollable height grows, so stay pinned to the bottom.
        let scroller = list
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>();
        if let Some(scroller) = scroller {
            scroller.vadjustment().connect_changed(|adj| {
                adj.set_value(adj.upper() - adj.page_size());
            });
        }
        Self { list }
    }

    pub fn clear(&self) {
        self.list.remove_all();
    }

    /// Starts the section of a new compilation, with its time stamp.
    pub fn begin(&self, title: &str) {
        let time = glib::DateTime::now_local()
            .and_then(|now| now.format("%H:%M:%S"))
            .map(|s| s.to_string())
            .unwrap_or_default();
        let label = gtk::Label::builder()
            .label(format!("{time} · {title}"))
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["heading", "log-header"])
            .build();
        let row = gtk::ListBoxRow::builder()
            .child(&label)
            .activatable(false)
            .selectable(false)
            .build();
        self.push(&row);
    }

    pub fn append_diagnostic(&self, diag: &Diagnostic) {
        let location = match (&diag.program, &diag.file, diag.line) {
            (prog, Some(file), Some(line)) => Some(match prog {
                Some(prog) => format!("{prog} · {file}:{line}"),
                None => format!("{file}:{line}"),
            }),
            (Some(prog), _, Some(line)) => Some(format!("{prog} · línea {line}")),
            (Some(prog), _, None) => Some(prog.clone()),
            (None, Some(file), None) => Some(file.clone()),
            (None, _, _) => None,
        };
        self.append(diag.severity, &diag.message, location.as_deref());
    }

    /// Adds a message produced by Myne itself (not by groff).
    pub fn append(&self, severity: Severity, title: &str, subtitle: Option<&str>) {
        let row = adw::ActionRow::builder()
            .use_markup(false)
            .title(title)
            .title_selectable(true)
            .activatable(false)
            .build();
        if let Some(subtitle) = subtitle {
            row.set_subtitle(subtitle);
        }
        let icon = match severity {
            Severity::Error => Some(("dialog-error-symbolic", "error", "Error")),
            Severity::Warning => Some(("dialog-warning-symbolic", "warning", "Aviso")),
            Severity::Info => None,
        };
        match icon {
            Some((name, class, tooltip)) => {
                let image = gtk::Image::builder()
                    .icon_name(name)
                    .css_classes([class])
                    .tooltip_text(tooltip)
                    .build();
                row.add_prefix(&image);
            }
            None => {
                // Keep the text aligned with the rows that have an icon.
                let spacer = gtk::Image::builder().pixel_size(16).build();
                row.add_prefix(&spacer);
                row.add_css_class("dim-label");
            }
        }
        self.push(&row);
    }

    fn push(&self, row: &impl IsA<gtk::Widget>) {
        self.list.append(row);
    }
}
