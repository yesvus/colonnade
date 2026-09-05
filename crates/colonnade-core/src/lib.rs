//! Toolkit-independent engine behind Colonnade's niri tab strip: the niri
//! IPC client and snapshot model, column grouping and tab-width math,
//! visible-slice anchoring, and the collapsed-workspace marker glyph
//! vocabulary. No GTK, no iced -- see the `colonnade` crate (GTK/Waybar)
//! and Lumen's iced module for the toolkit-specific renderers built on
//! top of this.

pub mod column;
pub mod error;
pub mod glyph;
pub mod niri;
pub mod slice;
