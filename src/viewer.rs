// SPDX-License-Identifier: GPL-3.0-or-later
//! `MynePdfView`: the PDF preview.
//!
//! Pages are laid out in one column, one `DrawingArea` each. Only pages near
//! the visible area are rendered with Poppler, one per idle iteration, into a
//! per-page cache of Cairo surfaces at the current zoom and screen scale.
//! A stale surface (older zoom or older PDF) keeps being drawn, scaled, until
//! its replacement is ready, so reloading never flashes blank pages.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib::subclass::Signal;
use gtk::{gio, glib};

const MARGIN: i32 = 12;
const SPACING: i32 = 12;
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 4.0;
const ZOOM_STEP: f64 = 1.2;
/// 100 % zoom shows one PDF point as 96/72 logical pixels.
const PX_PER_PT_AT_100: f64 = 96.0 / 72.0;
/// Upper bound for one cached surface (pixels), to cap memory at high zoom.
const MAX_SURFACE_PIXELS: f64 = 32_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    Fixed(f64),
}

struct Cached {
    surface: cairo::ImageSurface,
    /// Device pixels per PDF point that were requested for this surface.
    scale: f64,
    doc_generation: u64,
}

struct PageSlot {
    width_pt: f64,
    height_pt: f64,
    area: gtk::DrawingArea,
}

struct State {
    document: Option<poppler::Document>,
    doc_generation: u64,
    pages: Vec<PageSlot>,
    cache: HashMap<usize, Cached>,
    zoom: Zoom,
    /// Logical pixels per PDF point currently in effect.
    px_per_pt: f64,
    hadjustment: Option<gtk::Adjustment>,
    vadjustment: Option<gtk::Adjustment>,
    pending: BTreeSet<usize>,
    idle: Option<glib::SourceId>,
    /// Scroll position to restore once the new layout is allocated:
    /// page index and offset within that page (0..1).
    anchor: Option<(usize, f64)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            document: None,
            doc_generation: 0,
            pages: Vec::new(),
            cache: HashMap::new(),
            zoom: Zoom::FitWidth,
            px_per_pt: PX_PER_PT_AT_100,
            hadjustment: None,
            vadjustment: None,
            pending: BTreeSet::new(),
            idle: None,
            anchor: None,
        }
    }
}

impl State {
    fn page_height(&self, index: usize) -> i32 {
        self.pages
            .get(index)
            .map(|p| px(p.height_pt * self.px_per_pt))
            .unwrap_or(0)
    }

    fn page_top(&self, index: usize) -> f64 {
        let above: i32 = (0..index).map(|i| self.page_height(i) + SPACING).sum();
        f64::from(MARGIN + above)
    }

    fn total_height(&self) -> f64 {
        let n = self.pages.len();
        if n == 0 {
            return 0.0;
        }
        self.page_top(n - 1) + f64::from(self.page_height(n - 1) + MARGIN)
    }

    /// Index of the page at vertical position `y`.
    fn page_at(&self, y: f64) -> Option<usize> {
        if self.pages.is_empty() {
            return None;
        }
        let mut top = f64::from(MARGIN);
        for i in 0..self.pages.len() {
            let bottom = top + f64::from(self.page_height(i) + SPACING);
            if y < bottom {
                return Some(i);
            }
            top = bottom;
        }
        Some(self.pages.len() - 1)
    }

    fn current_anchor(&self) -> Option<(usize, f64)> {
        let vadj = self.vadjustment.as_ref()?;
        let y = vadj.value();
        let index = self.page_at(y)?;
        let height = f64::from(self.page_height(index).max(1));
        let fraction = ((y - self.page_top(index)) / height).clamp(0.0, 1.0);
        Some((index, fraction))
    }

    /// Pages intersecting the viewport, extended by half a screen each way.
    fn visible_range(&self) -> Option<(usize, usize)> {
        let vadj = self.vadjustment.as_ref()?;
        let extra = vadj.page_size() * 0.5;
        let first = self.page_at((vadj.value() - extra).max(0.0))?;
        let last = self.page_at(vadj.value() + vadj.page_size() + extra)?;
        Some((first, last))
    }

    fn is_fresh(&self, index: usize, scale: f64) -> bool {
        self.cache.get(&index).is_some_and(|c| {
            c.doc_generation == self.doc_generation && (c.scale - scale).abs() < 1e-6
        })
    }
}

fn px(value: f64) -> i32 {
    // Page sizes are small positive numbers; the cast cannot overflow.
    value.round().max(1.0) as i32
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PdfView {
        pub(super) state: RefCell<State>,
        pub(super) empty: RefCell<Option<adw::StatusPage>>,
        pub(super) last_fit_width: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PdfView {
        const NAME: &'static str = "MynePdfView";
        type Type = super::PdfView;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for PdfView {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Vertical);
            obj.set_spacing(SPACING);
            obj.set_margin_top(MARGIN);
            obj.set_margin_bottom(MARGIN);
            obj.set_margin_start(MARGIN);
            obj.set_margin_end(MARGIN);

            let empty = adw::StatusPage::builder()
                .icon_name("x-office-document-symbolic")
                .title(gettext("No preview"))
                .description(gettext(
                    "Save the document or press F5 to generate the PDF.",
                ))
                .vexpand(true)
                .build();
            empty.add_css_class("compact");
            obj.append(&empty);
            self.empty.replace(Some(empty));

            obj.connect_scale_factor_notify(|view| view.request_visible());
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("view-changed").build()])
        }

        fn dispose(&self) {
            if let Some(id) = self.state.borrow_mut().idle.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for PdfView {}
    impl BoxImpl for PdfView {}
}

glib::wrapper! {
    pub struct PdfView(ObjectSubclass<imp::PdfView>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PdfView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PdfView {
    /// Emitted when zoom, page count or current page change.
    pub fn connect_view_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("view-changed", false, move |args| {
            if let Some(view) = args.first().and_then(|v| v.get::<Self>().ok()) {
                f(&view);
            }
            None
        })
    }

    fn emit_view_changed(&self) {
        self.emit_by_name::<()>("view-changed", &[]);
    }

    /// Connects the view to the scrolled window that contains it.
    pub fn set_scroller(&self, scroller: &gtk::ScrolledWindow) {
        scroller.add_css_class("pdf-view");
        let hadj = scroller.hadjustment();
        let vadj = scroller.vadjustment();
        {
            let mut state = self.imp().state.borrow_mut();
            state.hadjustment = Some(hadj.clone());
            state.vadjustment = Some(vadj.clone());
        }

        vadj.connect_value_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| {
                view.request_visible();
                view.emit_view_changed();
            }
        ));
        vadj.connect_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_| view.apply_anchor()
        ));
        hadj.connect_page_size_notify(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |adj| {
                let width = adj.page_size();
                let imp = view.imp();
                let fit = imp.state.borrow().zoom == Zoom::FitWidth;
                if fit && (width - imp.last_fit_width.get()).abs() >= 1.0 {
                    // Resizing pages while GTK is still allocating can leave
                    // the old width in place, so wait for the next idle.
                    glib::idle_add_local_once(glib::clone!(
                        #[weak]
                        view,
                        move || view.relayout(true)
                    ));
                }
            }
        ));

        // Ctrl + wheel zooms instead of scrolling.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = view)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, _dx, dy| {
                if !controller
                    .current_event_state()
                    .contains(gtk::gdk::ModifierType::CONTROL_MASK)
                {
                    return glib::Propagation::Proceed;
                }
                view.zoom_by(ZOOM_STEP.powf(-dy));
                glib::Propagation::Stop
            }
        ));
        scroller.add_controller(scroll);
    }

    /// Shows the PDF in `bytes`. If a document was already shown, the page,
    /// scroll offset and zoom are kept.
    pub fn load_pdf(&self, bytes: &glib::Bytes) -> Result<(), glib::Error> {
        let document = poppler::Document::from_bytes(bytes, None)?;
        let n_pages = usize::try_from(document.n_pages()).unwrap_or(0);
        let sizes: Vec<(f64, f64)> = (0..n_pages)
            .map(|i| {
                i32::try_from(i)
                    .ok()
                    .and_then(|i| document.page(i))
                    .map(|p| p.size())
                    .unwrap_or((612.0, 792.0))
            })
            .collect();

        let mut new_areas = Vec::new();
        let mut removed = Vec::new();
        {
            let mut state = self.imp().state.borrow_mut();
            let had_document = state.document.is_some();
            state.anchor = if had_document {
                state.current_anchor()
            } else {
                None
            };
            state.document = Some(document);
            state.doc_generation += 1;
            state.cache.retain(|&i, _| i < n_pages);
            while state.pages.len() > n_pages {
                if let Some(slot) = state.pages.pop() {
                    removed.push(slot.area);
                }
            }
            for (i, (width_pt, height_pt)) in sizes.iter().copied().enumerate() {
                if let Some(slot) = state.pages.get_mut(i) {
                    slot.width_pt = width_pt;
                    slot.height_pt = height_pt;
                } else {
                    let area = self.new_page_area(i);
                    new_areas.push(area.clone());
                    state.pages.push(PageSlot {
                        width_pt,
                        height_pt,
                        area,
                    });
                }
            }
        }
        for area in removed {
            self.remove(&area);
        }
        for area in new_areas {
            self.append(&area);
        }
        if let Some(empty) = self.imp().empty.borrow().as_ref() {
            empty.set_visible(false);
        }
        self.relayout(false);
        Ok(())
    }

    /// Forgets the document and shows the empty state.
    pub fn clear(&self) {
        let removed: Vec<gtk::DrawingArea> = {
            let mut state = self.imp().state.borrow_mut();
            state.document = None;
            state.cache.clear();
            state.pending.clear();
            state.anchor = None;
            state.pages.drain(..).map(|slot| slot.area).collect()
        };
        for area in removed {
            self.remove(&area);
        }
        if let Some(empty) = self.imp().empty.borrow().as_ref() {
            empty.set_visible(true);
        }
        self.emit_view_changed();
    }

    pub fn has_document(&self) -> bool {
        self.imp().state.borrow().document.is_some()
    }

    pub fn n_pages(&self) -> usize {
        self.imp().state.borrow().pages.len()
    }

    /// Current page, 1-based (0 if there is no document).
    pub fn current_page(&self) -> usize {
        let state = self.imp().state.borrow();
        let Some(vadj) = state.vadjustment.as_ref() else {
            return 0;
        };
        // The page occupying the upper third of the viewport.
        state
            .page_at(vadj.value() + vadj.page_size() / 3.0)
            .map_or(0, |i| i + 1)
    }

    pub fn zoom_percent(&self) -> u32 {
        let px_per_pt = self.imp().state.borrow().px_per_pt;
        // Zoom is clamped to a small range; the cast cannot overflow.
        (px_per_pt / PX_PER_PT_AT_100 * 100.0).round() as u32
    }

    pub fn zoom_in(&self) {
        self.zoom_by(ZOOM_STEP);
    }

    pub fn zoom_out(&self) {
        self.zoom_by(1.0 / ZOOM_STEP);
    }

    pub fn zoom_fit(&self) {
        self.imp().state.borrow_mut().zoom = Zoom::FitWidth;
        self.relayout(true);
    }

    fn zoom_by(&self, factor: f64) {
        {
            let mut state = self.imp().state.borrow_mut();
            let current = state.px_per_pt / PX_PER_PT_AT_100;
            state.zoom = Zoom::Fixed((current * factor).clamp(MIN_ZOOM, MAX_ZOOM));
        }
        self.relayout(true);
    }

    pub fn previous_page(&self) {
        let current = self.current_page();
        if current > 1 {
            self.scroll_to_page(current - 2);
        }
    }

    pub fn next_page(&self) {
        let current = self.current_page();
        if current > 0 && current < self.n_pages() {
            self.scroll_to_page(current);
        }
    }

    fn scroll_to_page(&self, index: usize) {
        let (vadj, target) = {
            let state = self.imp().state.borrow();
            let Some(vadj) = state.vadjustment.clone() else {
                return;
            };
            (vadj, state.page_top(index) - f64::from(SPACING) / 2.0)
        };
        vadj.set_value(target);
    }

    fn new_page_area(&self, index: usize) -> gtk::DrawingArea {
        let area = gtk::DrawingArea::builder()
            .halign(gtk::Align::Center)
            .css_classes(["pdf-page"])
            .build();
        area.set_draw_func(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, cr, width, height| view.draw_page(index, cr, width, height)
        ));
        area
    }

    fn draw_page(&self, index: usize, cr: &cairo::Context, width: i32, height: i32) {
        let needs_render = {
            let state = self.imp().state.borrow();
            if let Some(cached) = state.cache.get(&index) {
                let sw = f64::from(cached.surface.width().max(1));
                let sh = f64::from(cached.surface.height().max(1));
                cr.scale(f64::from(width) / sw, f64::from(height) / sh);
                if cr.set_source_surface(&cached.surface, 0.0, 0.0).is_ok() {
                    cr.source().set_filter(cairo::Filter::Good);
                    let _ = cr.paint();
                }
            }
            !state.is_fresh(index, state.px_per_pt * f64::from(self.scale_factor()))
        };
        if needs_render {
            self.request_page(index);
        }
    }

    /// Recomputes page sizes for the current zoom. With `keep_position`,
    /// the page and offset at the top of the viewport are preserved.
    fn relayout(&self, keep_position: bool) {
        {
            let mut state = self.imp().state.borrow_mut();
            if keep_position && state.anchor.is_none() {
                state.anchor = state.current_anchor();
            }
            state.px_per_pt = match state.zoom {
                Zoom::Fixed(zoom) => zoom * PX_PER_PT_AT_100,
                Zoom::FitWidth => {
                    let viewport = state.hadjustment.as_ref().map_or(0.0, |a| a.page_size());
                    self.imp().last_fit_width.set(viewport);
                    let widest = state.pages.iter().map(|p| p.width_pt).fold(0.0, f64::max);
                    let available = viewport - f64::from(2 * MARGIN);
                    if widest > 0.0 && available > 50.0 {
                        (available / widest)
                            .clamp(MIN_ZOOM * PX_PER_PT_AT_100, MAX_ZOOM * PX_PER_PT_AT_100)
                    } else {
                        PX_PER_PT_AT_100
                    }
                }
            };
            let px_per_pt = state.px_per_pt;
            for slot in &state.pages {
                slot.area.set_content_width(px(slot.width_pt * px_per_pt));
                slot.area.set_content_height(px(slot.height_pt * px_per_pt));
                slot.area.queue_draw();
            }
        }
        // If the size did not change, no `changed` signal will come.
        self.apply_anchor();
        self.request_visible();
        self.emit_view_changed();
    }

    /// Restores the saved scroll anchor once the adjustment matches the
    /// new layout.
    fn apply_anchor(&self) {
        let (vadj, target) = {
            let mut state = self.imp().state.borrow_mut();
            let Some((index, fraction)) = state.anchor else {
                return;
            };
            let Some(vadj) = state.vadjustment.clone() else {
                return;
            };
            // The viewport's upper bound is never below its own height.
            let expected = state.total_height().max(vadj.page_size());
            if (vadj.upper() - expected).abs() > 2.0 {
                return;
            }
            state.anchor = None;
            let index = index.min(state.pages.len().saturating_sub(1));
            let target = state.page_top(index) + fraction * f64::from(state.page_height(index));
            (vadj, target)
        };
        vadj.set_value(target);
        self.request_visible();
        self.emit_view_changed();
    }

    fn request_visible(&self) {
        let pages: Vec<usize> = {
            let state = self.imp().state.borrow();
            match state.visible_range() {
                Some((first, last)) => (first..=last).collect(),
                None => Vec::new(),
            }
        };
        for index in pages {
            self.request_page(index);
        }
    }

    fn request_page(&self, index: usize) {
        let scale_factor = f64::from(self.scale_factor());
        let mut state = self.imp().state.borrow_mut();
        if state.document.is_none() || state.is_fresh(index, state.px_per_pt * scale_factor) {
            return;
        }
        state.pending.insert(index);
        if state.idle.is_none() {
            let id = glib::idle_add_local(glib::clone!(
                #[weak(rename_to = view)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || view.render_next()
            ));
            state.idle = Some(id);
        }
    }

    /// Renders one pending page that is still near the viewport.
    fn render_next(&self) -> glib::ControlFlow {
        let scale_factor = f64::from(self.scale_factor());
        let mut state = self.imp().state.borrow_mut();
        let range = state.visible_range();
        while let Some(index) = state.pending.pop_first() {
            let near = range.is_some_and(|(first, last)| index >= first && index <= last);
            let scale = state.px_per_pt * scale_factor;
            if !near || state.is_fresh(index, scale) {
                continue;
            }
            let (Some(document), Some(slot)) = (state.document.as_ref(), state.pages.get(index))
            else {
                continue;
            };
            let rendered = i32::try_from(index)
                .ok()
                .and_then(|i| document.page(i))
                .and_then(|page| render_page(&page, slot.width_pt, slot.height_pt, scale));
            let area = slot.area.clone();
            if let Some(surface) = rendered {
                let doc_generation = state.doc_generation;
                state.cache.insert(
                    index,
                    Cached {
                        surface,
                        scale,
                        doc_generation,
                    },
                );
            }
            // Drop surfaces far from the viewport.
            if let Some((first, last)) = range {
                state.cache.retain(|&i, _| i + 3 >= first && i <= last + 3);
            }
            drop(state);
            area.queue_draw();
            return glib::ControlFlow::Continue;
        }
        state.idle = None;
        glib::ControlFlow::Break
    }
}

fn render_page(
    page: &poppler::Page,
    width_pt: f64,
    height_pt: f64,
    scale: f64,
) -> Option<cairo::ImageSurface> {
    let mut scale = scale;
    let pixels = width_pt * height_pt * scale * scale;
    if pixels > MAX_SURFACE_PIXELS {
        scale *= (MAX_SURFACE_PIXELS / pixels).sqrt();
    }
    let surface = cairo::ImageSurface::create(
        cairo::Format::Rgb24,
        px(width_pt * scale),
        px(height_pt * scale),
    )
    .ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().ok()?;
        cr.scale(scale, scale);
        page.render(&cr);
    }
    surface.flush();
    Some(surface)
}

/// Reads a PDF file fully into memory, so the viewer never depends on the
/// file staying on disk.
pub async fn read_pdf(file: &gio::File) -> Result<glib::Bytes, glib::Error> {
    let (contents, _etag) = file.load_contents_future().await?;
    Ok(glib::Bytes::from_owned(contents))
}
