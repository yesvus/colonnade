use std::collections::HashMap;

use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Deserializer};

/// The taskbar configuration.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    apps: HashMap<String, Vec<AppConfig>>,
    #[serde(default)]
    notifications: Notifications,
    #[serde(default)]
    show_all_outputs: bool,
    #[serde(default)]
    layout: Layout,
}

/// Layout tunables -- these started as hardcoded constants (`WIDTH_SCALE_PX`
/// etc. in `column.rs`/`workspace_slot.rs`/`glyph.rs`) that needed a
/// rebuild to adjust, which got old fast during the amount of by-eye
/// tuning this bar has needed. Exposed here instead, under a `layout` key
/// in the Waybar module config, same place `show_all_outputs` already
/// lives -- e.g.:
///
/// ```jsonc
/// "cffi/colonnade": {
///     "module_path": "...",
///     "layout": { "max_group_width_px": 700 }
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct Layout {
    /// Each column's tab width is `width_fraction * tab_width_scale_px`,
    /// computed independently per column (see BEHAVIOR.md and column.rs's
    /// doc comment on why this isn't just `width_fraction * output_width`,
    /// and why it's never normalized against sibling tabs).
    #[serde(default = "default_tab_width_scale_px")]
    tab_width_scale_px: f64,
    /// Floor so a tiny or momentarily-zero width_fraction never produces a
    /// degenerate, barely-clickable tab.
    #[serde(default = "default_min_tab_width_px")]
    min_tab_width_px: i32,
    /// Real pixel budget for the visible tab group. However many tabs fit
    /// at their true (unshrunk) width is how many show -- not a fixed tab
    /// count, since that could still let a few wide tabs blow past the
    /// screen (see workspace_slot.rs).
    #[serde(default = "default_max_group_width_px")]
    max_group_width_px: i32,
    /// Caps both collapsed-marker and overflow-tick glyph strings at this
    /// many characters (with `…` when truncated) -- without a cap, a
    /// workspace with enough windows renders a marker/tick string as wide
    /// as an uncapped tab group would have been.
    #[serde(default = "default_max_overflow_glyphs")]
    max_overflow_glyphs: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            tab_width_scale_px: default_tab_width_scale_px(),
            min_tab_width_px: default_min_tab_width_px(),
            max_group_width_px: default_max_group_width_px(),
            max_overflow_glyphs: default_max_overflow_glyphs(),
        }
    }
}

fn default_tab_width_scale_px() -> f64 {
    260.0
}

fn default_min_tab_width_px() -> i32 {
    40
}

fn default_max_group_width_px() -> i32 {
    // Bumped from the original 520 -- still bounded (the whole reason
    // this replaced the old GtkScrolledWindow approach), just with more
    // headroom before it kicks in.
    620
}

fn default_max_overflow_glyphs() -> usize {
    10
}

#[derive(Debug, Deserialize)]
pub struct Notifications {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    map_app_ids: HashMap<String, String>,
    #[serde(default = "default_true")]
    use_desktop_entry: bool,
    #[serde(default)]
    use_fuzzy_matching: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            map_app_ids: Default::default(),
            use_desktop_entry: true,
            use_fuzzy_matching: Default::default(),
        }
    }
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Returns all possible CSS classes that a particular application might have set.
    pub fn app_classes(&self, app_id: &str) -> Vec<&str> {
        self.apps
            .get(app_id)
            .map(|configs| {
                configs
                    .iter()
                    .map(|config| config.class.as_str())
                    .collect_vec()
            })
            .unwrap_or_default()
    }

    /// Returns the actual CSS classes that should be set for the given application and title.
    pub fn app_matches<'a>(
        &'a self,
        app_id: &str,
        title: &'a str,
    ) -> Box<dyn Iterator<Item = &'a str> + 'a> {
        match self.apps.get(app_id) {
            Some(configs) => Box::new(
                configs
                    .iter()
                    .filter(|config| config.re.is_match(title))
                    .map(|config| config.class.as_str()),
            ),
            None => Box::new(std::iter::empty()),
        }
    }

    /// Returns true if notification support is enabled.
    pub fn notifications_enabled(&self) -> bool {
        self.notifications.enabled
    }

    /// Returns any mapping that might exist for this app ID.
    pub fn notifications_app_map(&self, app_id: &str) -> Option<&'_ str> {
        self.notifications
            .map_app_ids
            .get(app_id)
            .map(String::as_str)
    }

    /// Returns true if notification support should use the desktop entry as a
    /// fallback.
    pub fn notifications_use_desktop_entry(&self) -> bool {
        self.notifications.use_desktop_entry
    }

    pub fn notifications_use_fuzzy_matching(&self) -> bool {
        self.notifications.use_fuzzy_matching
    }

    pub fn show_all_outputs(&self) -> bool {
        self.show_all_outputs
    }

    pub fn tab_width_scale_px(&self) -> f64 {
        self.layout.tab_width_scale_px
    }

    pub fn min_tab_width_px(&self) -> i32 {
        self.layout.min_tab_width_px
    }

    pub fn max_group_width_px(&self) -> i32 {
        self.layout.max_group_width_px
    }

    pub fn max_overflow_glyphs(&self) -> usize {
        self.layout.max_overflow_glyphs
    }
}

#[derive(Deserialize, Debug)]
struct AppConfig {
    #[serde(rename = "match", deserialize_with = "deserialise_regex")]
    re: Regex,
    class: String,
}

fn deserialise_regex<'de, D>(de: D) -> Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    Regex::new(&String::deserialize(de)?).map_err(serde::de::Error::custom)
}
