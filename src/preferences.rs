// SPDX-License-Identifier: GPL-3.0-or-later
//! Preferences dialog. Changes apply immediately and are saved to the
//! configuration file.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::config::Config;
use crate::window::Window;

/// Stores the value of a switch in the configuration.
type Setter = fn(&mut Config, bool);

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/calmecac/Myne/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub compile_on_save: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub command_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub tbl_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub eqn_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pic_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub refer_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub show_log_on_error: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "MynePreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesDialog {}
    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PreferencesDialog {
    /// Creates the dialog showing the configuration of `window`.
    pub fn new(window: &Window) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        let config = window.config();
        imp.compile_on_save.set_active(config.compile_on_save);
        imp.command_row.set_text(&config.command);
        imp.tbl_row.set_active(config.tbl);
        imp.eqn_row.set_active(config.eqn);
        imp.pic_row.set_active(config.pic);
        imp.refer_row.set_active(config.refer);
        imp.show_log_on_error.set_active(config.show_log_on_error);

        let switches: [(&adw::SwitchRow, Setter); 6] = [
            (&imp.compile_on_save, |c, v| c.compile_on_save = v),
            (&imp.tbl_row, |c, v| c.tbl = v),
            (&imp.eqn_row, |c, v| c.eqn = v),
            (&imp.pic_row, |c, v| c.pic = v),
            (&imp.refer_row, |c, v| c.refer = v),
            (&imp.show_log_on_error, |c, v| c.show_log_on_error = v),
        ];
        for (row, set) in switches {
            row.connect_active_notify(glib::clone!(
                #[weak]
                window,
                move |row| {
                    let active = row.is_active();
                    window.update_config(|c| set(c, active));
                }
            ));
        }
        imp.command_row.connect_changed(glib::clone!(
            #[weak]
            window,
            move |row| {
                let command = row.text().trim().to_owned();
                window.update_config(|c| c.command = command);
            }
        ));
        dialog
    }
}
