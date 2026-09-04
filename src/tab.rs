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
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
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
            use waybar_cffi::gtk::gdk::{BUTTON_MIDDLE, BUTTON_SECONDARY, EventType};

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
                BUTTON_MIDDLE | BUTTON_SECONDARY => {
                    if let Err(e) = state.niri().close_window(window_id) {
                        tracing::warn!(%e, id = window_id, "error closing window");
                    }
                    glib::Propagation::Stop
                }
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
    const APP_ICONS: &[(&str, &str)] = &[
        ("firefox", "󰈹"),
        ("chromium", "󰊯"),
        ("google-chrome", "󰊯"),
        ("ghostty", ""),
        ("foot", ""),
        ("alacritty", ""),
        ("kitty", ""),
        ("dev.zed.zed", "󰨞"),
        ("code", "󰨞"),
        ("vscodium", "󰨞"),
        ("discord", "󰙯"),
        ("vesktop", "󰙯"),
        ("telegram", "󰈰"),
        ("spotify", "󰓇"),
        ("slack", "󰒱"),
        ("obsidian", "󰠮"),
        ("nemo", "󰉋"),
        ("thunar", "󰉋"),
        ("nautilus", "󰉋"),
        ("btop", "󰌓"),
    ];
    const DEFAULT_ICON: &str = "";

    let app = app_id.unwrap_or_default().to_lowercase();
    let t = title.unwrap_or_default().to_lowercase();
    APP_ICONS
        .iter()
        .find(|(key, _)| app.contains(key) || t.contains(key))
        .map(|(_, icon)| *icon)
        .unwrap_or(DEFAULT_ICON)
}
