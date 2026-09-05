//! A single tab: one niri column, rendered inside the bloomed workspace's
//! group. Rewrite of upstream's `Button` -- adds a visible title label
//! (upstream only showed a tooltip), animated width (see `animate.rs`),
//! and Colonnade's click semantics instead of upstream's single-click-only
//! (single = focus, double = focus + maximize, middle/right = close, per
//! BEHAVIOR.md).
//!
//! Real per-app icon loading (upstream's async Pixbuf cache in `icon.rs`)
//! is deferred past Phase 1 -- this uses the same Nerd Font glyph table
//! the Python daemon it replaces already used, which is simpler and
//! visually matches what's already shipping. `icon.rs`'s cache is kept
//! for when that lands.

use std::fmt::Debug;

use waybar_cffi::gtk::{
    self,
    glib::Cast,
    prelude::{BoxExt, ButtonExt, ContainerExt, LabelExt, StyleContextExt, WidgetExt},
};

use colonnade_core::column::Column;

use crate::{animate::Animator, state::State};

/// Pixels between a tab's icon and its title. 6px, matching the gap the
/// workspace number keeps from whatever follows it, so the two spacings on
/// the strip are the same number rather than two independent guesses.
const ICON_GAP_PX: i32 = 6;

pub struct Tab {
    button: gtk::Button,
    icon: gtk::Label,
    label: gtk::Label,
    width: Animator,
    window_id: u64,
    state: State,
}

impl Debug for Tab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tab")
            .field("window_id", &self.window_id)
            .finish()
    }
}

impl Tab {
    pub fn new(state: &State, column: &Column<'_>) -> Self {
        let state = state.clone();

        let button = gtk::Button::new();
        button.style_context().add_class("colonnade-tab");
        // The real reason tabs looked tall. A GtkBox hands every child the
        // full cross-axis extent by default (valign: Fill), so each tab
        // button was stretched to the entire bar height regardless of what
        // it actually asked for -- measured: natural height 17px (13px of
        // text + 1px border + 1px margin, top and bottom), allocated 26px,
        // exactly the bar's own height, and every parent widget up to
        // waybar's toplevel allocated 26 too. That's why zeroing padding
        // and min-height, fixing selector specificity, and shrinking
        // font-size each only nibbled at it: they all shrink the *request*,
        // which was never what was being drawn. Center makes the button
        // take its natural height and sit centred in whatever bar height
        // is configured, which also makes vertical `padding` in the
        // stylesheet a working lever on pill height for the first time.
        button.set_valign(gtk::Align::Center);

        // Icon and title are two labels in a spaced box, not one string of
        // "icon + space + title". The space character was doing the
        // separating, and a space is only as wide as whichever font ends up
        // rendering it -- which changed under us twice already (Open Sans
        // took over the text, and the icons moved off the Mono Nerd Font
        // variant to stop being squeezed into one cell), each time silently
        // resizing a gap nobody had declared. GtkBox spacing states it in
        // pixels instead, and it can't be affected by a font change.
        let content = gtk::Box::new(gtk::Orientation::Horizontal, ICON_GAP_PX);

        let icon = gtk::Label::new(None);
        icon.style_context().add_class("colonnade-tab");
        // Left-aligned inside its own allocation, because `render` widens
        // that allocation to the glyph's real drawn width -- centring would
        // split the extra space either side and put the gap back in the
        // wrong place.
        icon.set_xalign(0.0);
        content.add(&icon);

        let label = gtk::Label::new(None);
        // The button's own "colonnade-tab" class controls its background/
        // border fine, but font-size didn't inherit down to this label
        // from it (found by looking at it: the workspace-number label,
        // which gets its class directly, shrank correctly; tab text,
        // styled only via its *parent* button's class, didn't move at
        // all). Putting the class on the label too makes lib.rs's
        // font-size CSS provider match it directly, no inheritance
        // required.
        label.style_context().add_class("colonnade-tab");
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        // Ellipsize alone doesn't stop the label from *requesting* its
        // full unellipsized text width during layout negotiation -- the
        // button's set_size_request below only sets a minimum, so a
        // longer title still wins and the explicit target width gets
        // silently ignored (found by actually looking at it: every tab
        // came out the same width, dominated by "Alacritty"'s natural
        // width, regardless of each column's real proportion). Capping
        // the label's own natural request to ~1 char forces it to defer
        // to the button's allocated width instead.
        label.set_max_width_chars(1);
        // Only the title ellipsizes and defers its width. The icon is one
        // glyph and must always be drawn in full, so it stays outside that.
        content.pack_start(&label, true, true, 0);
        button.add(&content);

        let window_id = column.window.id;
        let target_width = column.target_width_px;
        let height = state.config().tab_height_px();

        let tab = Self {
            button,
            icon,
            label,
            width: Animator::new(target_width as f64),
            window_id,
            state,
        };

        tab.connect_clicks();
        tab.button.set_size_request(target_width, height);
        tab.render(column);
        tab
    }

    pub fn widget(&self) -> &gtk::Button {
        &self.button
    }

    pub fn window_id(&self) -> u64 {
        self.window_id
    }

    /// Updates this tab's label and focus state for a new snapshot of the
    /// same column (same window id), and animates its width to the
    /// column's new target rather than snapping.
    pub fn update(&self, column: &Column<'_>) {
        self.render(column);

        let target = column.target_width_px as f64;
        if self.width.target() != target {
            let button = self.button.clone();
            // Height goes back in on every animation tick, not -1: a size
            // request is both dimensions at once, so passing -1 here would
            // hand the height back to GTK mid-animation and let the tab
            // snap to its natural size the first time it resized.
            let height = self.state.config().tab_height_px();
            self.width
                .to(self.button.upcast_ref(), target, move |value| {
                    button.set_size_request(value.round() as i32, height);
                });
        }
    }

    fn render(&self, column: &Column<'_>) {
        let title = column
            .window
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .or(column.window.app_id.as_deref())
            .unwrap_or("window");
        let icon = icon_glyph(column.window.app_id.as_deref(), Some(title));

        self.icon.set_text(icon);
        self.reserve_icon_width(icon);
        self.label.set_text(title);
        self.button.set_tooltip_text(Some(title));

        let context = self.button.style_context();
        if column.window.is_focused {
            context.add_class("focused");
        } else {
            context.remove_class("focused");
        }
    }

    /// Widens the icon label to the glyph's *drawn* width when that exceeds
    /// its advance width.
    ///
    /// Nerd Font icons overhang their own advance in the non-Mono variants:
    /// measured at 9pt, the terminal glyph advances 7px and inks 12px, so it
    /// spills 5px past the end of its own allocation. GTK sizes a label from
    /// advance width, so the icon was drawing straight over the start of the
    /// title -- the box spacing next to it was being consumed by overhang
    /// before it could separate anything, and with the old single-label
    /// "icon + space + title" the 3px space lost to that 5px outright and
    /// the two actually overlapped.
    ///
    /// Measured per render rather than hardcoded: the overhang differs per
    /// glyph (4px to 5px across this table) and scales with font size, so
    /// any constant here would be wrong for some icon at some size. Asking
    /// the label's own Pango context is right for every combination.
    fn reserve_icon_width(&self, icon: &str) {
        let (ink, logical) = self
            .icon
            .create_pango_layout(Some(icon))
            .pixel_extents();
        let drawn = ink.x() + ink.width();
        self.icon.set_size_request(drawn.max(logical.width()), -1);
    }

    fn connect_clicks(&self) {
        let state = self.state.clone();
        let window_id = self.window_id;

        self.button.connect_button_release_event(move |_, event| {
            use waybar_cffi::gtk::gdk::{BUTTON_MIDDLE, EventType};

            // Double-click delivers two release events; the second one
            // arrives as `DoubleButtonPress`'s matching release, which Gdk
            // reports via `event.event_type()` on the *press*, not the
            // release -- so double-click is handled in the button-press
            // handler below instead, and single-click logic here only
            // needs to ignore presses it doesn't care about.
            if event.event_type() != EventType::ButtonRelease {
                return glib::Propagation::Proceed;
            }

            match event.button() {
                BUTTON_MIDDLE => {
                    if let Err(e) = state.niri().close_window(window_id) {
                        tracing::warn!(%e, id = window_id, "error closing window");
                    }
                    glib::Propagation::Stop
                }
                // Right-click deliberately does nothing yet -- reserved
                // for a future context menu (rename, etc.), not close.
                _ => glib::Propagation::Proceed,
            }
        });

        let state = self.state.clone();
        self.button.connect_button_press_event(move |_, event| {
            use waybar_cffi::gtk::gdk::EventType;

            if event.event_type() == EventType::DoubleButtonPress {
                if let Err(e) = state.niri().maximize_window(window_id) {
                    tracing::warn!(%e, id = window_id, "error maximizing window");
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });

        let state = self.state.clone();
        self.button.connect_clicked(move |_| {
            if let Err(e) = state.niri().activate_window(window_id) {
                tracing::warn!(%e, id = window_id, "error focusing window");
            }
        });
    }
}

use waybar_cffi::gtk::glib;

fn icon_glyph(app_id: Option<&str>, title: Option<&str>) -> &'static str {
    // \u{...} escapes, not literal pasted glyphs: a previous version used
    // literal characters and every one in the U+E000-U+F8FF (BMP Private
    // Use Area) range silently came out empty on write, while ones above
    // U+F0000 (supplementary-plane PUA-A) survived intact -- e.g. the
    // shared terminal-app icon (U+F489) and the default fallback
    // (U+F2D0) both went missing, which is why no terminal ever showed an
    // icon. Escapes make the exact codepoint explicit and can't silently
    // drop the same way. Codepoints confirmed against the already-working
    // Python daemon's own icon table.
    const APP_ICONS: &[(&str, &str)] = &[
        ("firefox", "\u{f0239}"),
        ("chromium", "\u{f02af}"),
        ("google-chrome", "\u{f02af}"),
        ("ghostty", "\u{f489}"),
        ("foot", "\u{f489}"),
        ("alacritty", "\u{f489}"),
        ("kitty", "\u{f489}"),
        ("dev.zed.zed", "\u{f0a1e}"),
        ("code", "\u{f0a1e}"),
        ("vscodium", "\u{f0a1e}"),
        ("discord", "\u{f066f}"),
        ("vesktop", "\u{f066f}"),
        ("telegram", "\u{f0230}"),
        ("spotify", "\u{f04c7}"),
        ("slack", "\u{f04b1}"),
        ("obsidian", "\u{f082e}"),
        ("nemo", "\u{f024b}"),
        ("thunar", "\u{f024b}"),
        ("nautilus", "\u{f024b}"),
        ("btop", "\u{f0313}"),
    ];
    const DEFAULT_ICON: &str = "\u{f2d0}";

    let app = app_id.unwrap_or_default().to_lowercase();
    let t = title.unwrap_or_default().to_lowercase();
    APP_ICONS
        .iter()
        .find(|(key, _)| app.contains(key) || t.contains(key))
        .map(|(_, icon)| *icon)
        .unwrap_or(DEFAULT_ICON)
}
