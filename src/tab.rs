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
    prelude::{ButtonExt, ContainerExt, LabelExt, StyleContextExt, WidgetExt},
};

use crate::{animate::Animator, column::Column, state::State};

pub struct Tab {
    button: gtk::Button,
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
        button.add(&label);

        let window_id = column.window.id;
        let target_width = column.target_width_px;

        let tab = Self {
            button,
            label,
            width: Animator::new(target_width as f64),
            window_id,
            state,
        };

        tab.connect_clicks();
        tab.button.set_size_request(target_width, -1);
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
            self.width
                .to(self.button.upcast_ref(), target, move |value| {
                    button.set_size_request(value.round() as i32, -1);
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

        self.label.set_text(&format!("{icon} {title}"));
        self.button.set_tooltip_text(Some(title));

        let context = self.button.style_context();
        if column.window.is_focused {
            context.add_class("focused");
        } else {
            context.remove_class("focused");
        }
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
