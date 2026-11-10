// SPDX-License-Identifier: GPL-3.0-or-later
//! Main window, built from `data/ui/window.blp`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::compiler::{self, CompileRequest, Job, Outcome, Severity};
use crate::config::Config;
use crate::editor;
use crate::log_panel::LogPanel;
use crate::viewer::{self, PdfView};

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/calmecac/Myne/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub status_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub source_view: TemplateChild<sourceview5::View>,
        #[template_child]
        pub source_buffer: TemplateChild<sourceview5::Buffer>,
        #[template_child]
        pub position_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub pdf_scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub pdf_view: TemplateChild<PdfView>,
        #[template_child]
        pub zoom_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub page_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub log_list: TemplateChild<gtk::ListBox>,

        pub file: RefCell<Option<gio::File>>,
        pub config: RefCell<Config>,
        pub log: RefCell<Option<Rc<LogPanel>>>,
        pub job: RefCell<Option<Rc<Job>>>,
        pub generation: Cell<u64>,
        pub compiler_available: Cell<bool>,
        pub banner_title: RefCell<String>,
        pub force_close: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "MyneWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            PdfView::ensure_type();
            sourceview5::View::ensure_type();
            sourceview5::Buffer::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();
            if self.force_close.get() || !self.source_buffer.is_modified() {
                obj.cancel_compilation();
                return self.parent_close_request();
            }
            let obj = obj.clone();
            glib::spawn_future_local(async move {
                if obj.confirm_discard().await {
                    obj.imp().force_close.set(true);
                    obj.close();
                }
            });
            glib::Propagation::Stop
        }
    }

    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

/// Answer of the "unsaved changes" dialog.
enum Unsaved {
    Save,
    Discard,
    Cancel,
}

impl Window {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    fn setup(&self) {
        let imp = self.imp();
        imp.log
            .replace(Some(Rc::new(LogPanel::new(imp.log_list.get()))));
        imp.banner_title.replace(imp.banner.title().to_string());

        let (config, error) = Config::load();
        imp.config.replace(config);
        if let Some(error) = error {
            self.log_message(
                Severity::Warning,
                &format!("{error:#}"),
                Some("Se usan las preferencias por defecto"),
            );
        }

        self.setup_actions();

        imp.pdf_view.set_scroller(&imp.pdf_scroller);
        imp.pdf_view.connect_view_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |view| win.update_viewer_labels(view)
        ));
        self.update_viewer_labels(&imp.pdf_view);

        let buffer = imp.source_buffer.get();
        editor::follow_system_theme(&buffer);
        buffer.connect_modified_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_title()
        ));
        buffer.connect_cursor_position_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |buffer| {
                win.imp()
                    .position_label
                    .set_label(&editor::cursor_position_text(buffer));
            }
        ));

        self.update_title();
        self.check_compiler();
    }

    fn setup_actions(&self) {
        let simple = |name: &str, f: fn(&Self)| {
            gio::ActionEntry::builder(name)
                .activate(move |win: &Self, _, _| f(win))
                .build()
        };
        self.add_action_entries([
            simple("open", |win| {
                let win = win.clone();
                glib::spawn_future_local(async move { win.open_with_dialog().await });
            }),
            simple("save", |win| {
                let win = win.clone();
                glib::spawn_future_local(async move {
                    win.save().await;
                });
            }),
            simple("save-as", |win| {
                let win = win.clone();
                glib::spawn_future_local(async move {
                    win.save_as().await;
                });
            }),
            simple("compile", Self::compile),
            simple("zoom-in", |win| win.imp().pdf_view.zoom_in()),
            simple("zoom-out", |win| win.imp().pdf_view.zoom_out()),
            simple("zoom-fit", |win| win.imp().pdf_view.zoom_fit()),
            simple("prev-page", |win| win.imp().pdf_view.previous_page()),
            simple("next-page", |win| win.imp().pdf_view.next_page()),
            simple("clear-log", |win| {
                if let Some(log) = win.log() {
                    log.clear();
                }
            }),
            simple("hide-log", |win| win.set_log_visible(false)),
            simple("open-log", |win| win.set_log_visible(true)),
            gio::ActionEntry::builder("show-log")
                .state(false.to_variant())
                .activate(|_: &Self, action, _| {
                    let shown = action
                        .state()
                        .and_then(|s| s.get::<bool>())
                        .unwrap_or(false);
                    action.set_state(&(!shown).to_variant());
                })
                .build(),
        ]);
    }

    pub fn config(&self) -> Config {
        self.imp().config.borrow().clone()
    }

    /// Changes the configuration, saves it and applies it.
    pub fn update_config(&self, change: impl FnOnce(&mut Config)) {
        let (changed, result) = {
            let mut config = self.imp().config.borrow_mut();
            let before = config.clone();
            change(&mut config);
            let changed = *config != before;
            (changed, changed.then(|| config.save()))
        };
        if !changed {
            return;
        }
        if let Some(Err(error)) = result {
            self.log_message(
                Severity::Error,
                "No se pudieron guardar las preferencias",
                Some(&format!("{error:#}")),
            );
        }
        self.check_compiler();
    }

    fn log(&self) -> Option<Rc<LogPanel>> {
        self.imp().log.borrow().clone()
    }

    fn log_message(&self, severity: Severity, title: &str, subtitle: Option<&str>) {
        if let Some(log) = self.log() {
            log.append(severity, title, subtitle);
        }
    }

    fn set_log_visible(&self, visible: bool) {
        self.change_action_state("show-log", &visible.to_variant());
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            action.set_enabled(enabled);
        }
    }

    // ----- Title and labels -------------------------------------------------

    fn update_title(&self) {
        let imp = self.imp();
        let modified = imp.source_buffer.is_modified();
        let file = imp.file.borrow();
        let name = file
            .as_ref()
            .and_then(|f| f.basename())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Sin título".into());
        let marker = if modified { "• " } else { "" };
        imp.window_title.set_title(&format!("{marker}{name}"));
        let subtitle = file
            .as_ref()
            .and_then(|f| f.parent())
            .and_then(|p| p.path())
            .map(|p| tilde(&p))
            .unwrap_or_default();
        imp.window_title.set_subtitle(&subtitle);
        self.set_title(Some(&format!("{marker}{name} — Myne")));
    }

    fn update_viewer_labels(&self, view: &PdfView) {
        let imp = self.imp();
        imp.zoom_label
            .set_label(&format!("{} %", view.zoom_percent()));
        let pages = view.n_pages();
        if view.has_document() && pages > 0 {
            imp.page_label
                .set_label(&format!("{} / {}", view.current_page().max(1), pages));
        } else {
            imp.page_label.set_label("– / –");
        }
    }

    fn set_status(&self, name: &str, count: usize) {
        let imp = self.imp();
        imp.status_stack.set_visible_child_name(name);
        imp.status_count.set_visible(count > 0);
        imp.status_count.set_label(&count.to_string());
    }

    // ----- Files ------------------------------------------------------------

    fn source_path(&self) -> Option<PathBuf> {
        self.imp().file.borrow().as_ref().and_then(|f| f.path())
    }

    /// Asks what to do with unsaved changes. Returns `true` if the caller
    /// may go on (changes saved or discarded).
    pub async fn confirm_discard(&self) -> bool {
        if !self.imp().source_buffer.is_modified() {
            return true;
        }
        match self.ask_unsaved().await {
            Unsaved::Save => self.save().await,
            Unsaved::Discard => true,
            Unsaved::Cancel => false,
        }
    }

    async fn ask_unsaved(&self) -> Unsaved {
        let dialog = adw::AlertDialog::new(
            Some("¿Guardar los cambios?"),
            Some("Si no los guardas, los cambios se perderán."),
        );
        dialog.add_responses(&[
            ("cancel", "_Cancelar"),
            ("discard", "_Descartar"),
            ("save", "_Guardar"),
        ]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        match dialog.choose_future(Some(self)).await.as_str() {
            "save" => Unsaved::Save,
            "discard" => Unsaved::Discard,
            _ => Unsaved::Cancel,
        }
    }

    fn show_error(&self, heading: &str, body: &str) {
        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_response("close", "_Cerrar");
        dialog.present(Some(self));
    }

    fn file_filters() -> gio::ListStore {
        let mom = gtk::FileFilter::new();
        mom.set_name(Some("Documentos mom (*.mom)"));
        mom.add_pattern("*.mom");
        let text = gtk::FileFilter::new();
        text.set_name(Some("Archivos de texto"));
        text.add_mime_type("text/*");
        let all = gtk::FileFilter::new();
        all.set_name(Some("Todos los archivos"));
        all.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&mom);
        filters.append(&text);
        filters.append(&all);
        filters
    }

    async fn open_with_dialog(&self) {
        if !self.confirm_discard().await {
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title("Abrir documento")
            .modal(true)
            .filters(&Self::file_filters())
            .build();
        if let Some(folder) = self.imp().file.borrow().as_ref().and_then(|f| f.parent()) {
            dialog.set_initial_folder(Some(&folder));
        }
        // An error here means the user closed the dialog.
        if let Ok(file) = dialog.open_future(Some(self)).await {
            self.load_file(file).await;
        }
    }

    /// Opens `file`, asking first about unsaved changes.
    pub async fn open_file(&self, file: gio::File) {
        if self.confirm_discard().await {
            self.load_file(file).await;
        }
    }

    async fn load_file(&self, file: gio::File) {
        let bytes = match file.load_contents_future().await {
            Ok((bytes, _etag)) => bytes,
            Err(error) => {
                self.show_error("No se pudo abrir el archivo", error.message());
                return;
            }
        };
        let Ok(text) = String::from_utf8(bytes.to_vec()) else {
            self.show_error(
                "No se pudo abrir el archivo",
                "El archivo no está codificado en UTF-8.",
            );
            return;
        };

        self.cancel_compilation();
        let imp = self.imp();
        editor::set_text(&imp.source_buffer, &text);
        imp.file.replace(Some(file));
        self.update_title();
        // Keep the "pdfmom not found" warning visible.
        if imp.compiler_available.get() {
            imp.banner.set_revealed(false);
            self.set_status("ok", 0);
        }
        imp.source_view.grab_focus();

        // Show the PDF from a previous session, if there is one.
        imp.pdf_view.clear();
        if let Some(source) = self.source_path() {
            let pdf = gio::File::for_path(compiler::pdf_path(&source));
            if pdf.query_exists(gio::Cancellable::NONE) {
                self.show_pdf(&pdf).await;
            }
        }
    }

    /// Saves the document. Returns `true` on success.
    pub async fn save(&self) -> bool {
        let file = self.imp().file.borrow().clone();
        match file {
            Some(file) => self.write_to(&file).await,
            None => self.save_as().await,
        }
    }

    async fn save_as(&self) -> bool {
        let dialog = gtk::FileDialog::builder()
            .title("Guardar documento")
            .modal(true)
            .filters(&Self::file_filters())
            .build();
        match self.imp().file.borrow().as_ref() {
            Some(file) => dialog.set_initial_file(Some(file)),
            None => dialog.set_initial_name(Some("documento.mom")),
        }
        let Ok(file) = dialog.save_future(Some(self)).await else {
            return false;
        };
        if !self.write_to(&file).await {
            return false;
        }
        self.imp().file.replace(Some(file));
        self.update_title();
        true
    }

    async fn write_to(&self, file: &gio::File) -> bool {
        let buffer = self.imp().source_buffer.get();
        let text = editor::text(&buffer);
        let result = file
            .replace_contents_future(text.into_bytes(), None, false, gio::FileCreateFlags::NONE)
            .await;
        if let Err((_, error)) = result {
            self.show_error("No se pudo guardar el archivo", error.message());
            return false;
        }
        let is_current = self
            .imp()
            .file
            .borrow()
            .as_ref()
            .is_none_or(|f| f.equal(file));
        if is_current {
            buffer.set_modified(false);
        }
        self.update_title();
        if self.imp().config.borrow().compile_on_save {
            // Run after the caller has updated the current file (save as).
            let win = self.clone();
            glib::idle_add_local_once(move || win.compile());
        }
        true
    }

    // ----- Compilation ------------------------------------------------------

    /// Checks that the compiler is installed and enables or disables
    /// compilation accordingly.
    fn check_compiler(&self) {
        let imp = self.imp();
        let command = imp.config.borrow().command.clone();
        let found = compiler::find_program(&command).is_some();
        let was = imp.compiler_available.replace(found);
        self.set_action_enabled("compile", found);
        if found {
            imp.banner.set_title(&imp.banner_title.borrow());
            if !was {
                imp.banner.set_revealed(false);
            }
            return;
        }
        let program = glib::shell_parse_argv(command.as_str())
            .ok()
            .and_then(|argv| argv.first().map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "pdfmom".into());
        imp.banner.set_title(&format!(
            "No se encontró «{program}». Instala groff para poder compilar."
        ));
        imp.banner.set_revealed(true);
        self.set_status("error", 0);
        self.log_message(
            Severity::Error,
            &format!("No se encontró «{program}» en el PATH"),
            Some("Instala groff (por ejemplo: sudo apt install groff) o revisa el comando en Preferencias"),
        );
    }

    fn cancel_compilation(&self) {
        if let Some(job) = self.imp().job.take() {
            job.cancel();
        }
        // Results of the cancelled run are ignored.
        let imp = self.imp();
        imp.generation.set(imp.generation.get() + 1);
    }

    /// Compiles the saved file. A running compilation is cancelled first,
    /// so there is never more than one.
    fn compile(&self) {
        let imp = self.imp();
        if !imp.compiler_available.get() {
            self.check_compiler();
            return;
        }
        let Some(source) = self.source_path() else {
            self.log_message(
                Severity::Info,
                "Guarda el documento para poder compilarlo",
                None,
            );
            return;
        };
        let Some(log) = self.log() else {
            return;
        };

        if imp.job.borrow().is_some() {
            log.append(Severity::Info, "Compilación anterior cancelada", None);
        }
        self.cancel_compilation();
        let generation = imp.generation.get();
        let job = Rc::new(Job::new());
        imp.job.replace(Some(job.clone()));

        let request = CompileRequest {
            source: source.clone(),
            options: imp.config.borrow().compile_options(),
        };
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        log.begin(&format!("Compilando {name}"));
        self.set_status("busy", 0);

        let win = self.downgrade();
        glib::spawn_future_local(async move {
            let live = win.clone();
            let result = compiler::run(&request, &job, move |diag| {
                if let Some(win) = live.upgrade() {
                    if win.imp().generation.get() == generation {
                        log.append_diagnostic(diag);
                    }
                }
            })
            .await;
            let Some(win) = win.upgrade() else {
                return;
            };
            if win.imp().generation.get() != generation {
                return;
            }
            win.imp().job.replace(None);
            win.finish_compilation(result).await;
        });
    }

    async fn finish_compilation(&self, result: Result<Outcome, compiler::CompileError>) {
        let imp = self.imp();
        let show_log_on_error = imp.config.borrow().show_log_on_error;
        let report = match result {
            Ok(Outcome::Finished(report)) => report,
            Ok(Outcome::Cancelled) => return,
            Err(error) => {
                self.log_message(Severity::Error, &error.to_string(), None);
                self.set_status("error", 1);
                imp.banner.set_revealed(true);
                if show_log_on_error {
                    self.set_log_visible(true);
                }
                return;
            }
        };

        let seconds = report.elapsed.as_secs_f64();
        let counts = format!(
            "{} {}, {} {}",
            report.errors,
            if report.errors == 1 {
                "error"
            } else {
                "errores"
            },
            report.warnings,
            if report.warnings == 1 {
                "aviso"
            } else {
                "avisos"
            },
        );
        if report.success {
            self.log_message(
                if report.warnings > 0 {
                    Severity::Warning
                } else {
                    Severity::Info
                },
                &format!("PDF generado en {seconds:.1} s"),
                Some(&counts),
            );
            self.set_status(
                if report.warnings > 0 { "warning" } else { "ok" },
                report.warnings,
            );
            imp.banner.set_revealed(false);
            self.show_pdf(&gio::File::for_path(&report.pdf)).await;
        } else {
            let status = match report.exit_status {
                Some(0) if report.errors > 0 => counts,
                Some(0) => format!("{counts}; pdfmom no generó ningún PDF"),
                Some(code) => format!("{counts}; pdfmom terminó con código {code}"),
                None => format!("{counts}; pdfmom terminó por una señal"),
            };
            self.log_message(
                Severity::Error,
                "La compilación falló; se conserva el PDF anterior",
                Some(&status),
            );
            self.set_status("error", report.errors + report.warnings);
            imp.banner.set_revealed(true);
            if show_log_on_error {
                self.set_log_visible(true);
            }
        }
    }

    async fn show_pdf(&self, file: &gio::File) {
        let result = viewer::read_pdf(file).await;
        let shown = result.and_then(|bytes| self.imp().pdf_view.load_pdf(&bytes));
        if let Err(error) = shown {
            self.log_message(
                Severity::Error,
                "No se pudo mostrar el PDF",
                Some(error.message()),
            );
        }
    }
}

/// `/home/ana/Documents` → `~/Documents`.
fn tilde(path: &Path) -> String {
    let home = glib::home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
