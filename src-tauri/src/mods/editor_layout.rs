//! How the Modern window was left and how it is drawn: its own parts' folds,
//! sizes and places, its palette and its text size. Ours.
//!
//! The Modern window (src/argentum/modern/ModernWindow.tsx) is the only
//! arrangement: folders, the Navigator and History on the left, the grid or
//! the photo in the centre, the tools in one tabbed panel on the right. The
//! choice between it and Classic, and the Compact panels switch, are gone; an
//! older file's `editorLayout` and `compactSliders` keys are no longer read.
//!
//! Only the interface reads these; no photo renders differently. They live in
//! argentum-processing.json through `ag_settings`. Everything else the window
//! remembers (panel widths, which panels are folded, which adjustment sections
//! are open) is today's own workspace state, saved where it always was.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ag_settings;

const PANELS_KEY: &str = "editorLayoutPanels";
const THEME_KEY: &str = "modernTheme";
const TEXT_SIZE_KEY: &str = "modernTextSize";

/// What the Modern arrangement's own parts were left as. The interface
/// owns the tab ids, the widths and what they start as; this only keeps them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPanels {
    /// The Navigator section is open.
    pub navigator_open: bool,
    /// The tab open beside the adjustments, or none while the tab panel is
    /// folded.
    pub tab: Option<String>,
    /// The tab panel's width, in CSS pixels.
    pub tab_width_px: u16,
    /// The History section is open. Saved before it could fold: open.
    #[serde(default = "open")]
    pub history_open: bool,
    /// History's height in the left column, in CSS pixels; none until dragged.
    #[serde(default)]
    pub history_height_px: Option<u16>,
    /// The Navigator floats over the photo instead of sitting in the left column.
    #[serde(default)]
    pub navigator_floating: bool,
    /// Where the floating Navigator was put: its top-left corner, as fractions
    /// of the centre's width and height; none until moved.
    #[serde(default)]
    pub navigator_at: Option<[f32; 2]>,
}

fn open() -> bool {
    true
}

/// The arrangement's parts as last left, or `None` if never saved or not
/// readable as panels, for the interface to start them its own way.
pub fn panels(library: &Path) -> Option<LayoutPanels> {
    ag_settings::get(library, PANELS_KEY).and_then(|value| serde_json::from_value(value).ok())
}

/// Remember the arrangement's parts.
pub fn save_panels(library: &Path, panels: &LayoutPanels) -> Result<(), String> {
    let value = serde_json::to_value(panels).map_err(|e| e.to_string())?;
    ag_settings::set(library, PANELS_KEY, value)
}

/// The Modern window's palette. Crosses to the interface as its lowercase
/// name. Classic keeps the app's own theme setting and never reads this.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModernTheme {
    /// Lightroom Classic's neutral greys, with a lighter backdrop behind the photo.
    #[default]
    Grey,
    /// The same, darker.
    Deep,
    Light,
}

impl ModernTheme {
    /// The palette a name stands for; an unknown name is Grey.
    pub fn from_name(name: &str) -> Self {
        serde_json::from_value(Value::from(name)).unwrap_or_default()
    }
}

/// The chosen palette, or Grey.
pub fn theme(library: &Path) -> ModernTheme {
    ag_settings::get(library, THEME_KEY)
        .and_then(|value| value.as_str().map(ModernTheme::from_name))
        .unwrap_or_default()
}

/// Choose a palette.
pub fn save_theme(library: &Path, theme: ModernTheme) -> Result<(), String> {
    let value = serde_json::to_value(theme).map_err(|e| e.to_string())?;
    ag_settings::set(library, THEME_KEY, value)
}

/// How large the panels' text is drawn. Crosses to the interface as its
/// lowercase name; the interface owns what each step scales by.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl TextSize {
    /// The size a name stands for; an unknown name is Normal.
    pub fn from_name(name: &str) -> Self {
        serde_json::from_value(Value::from(name)).unwrap_or_default()
    }
}

/// The chosen text size, or Normal.
pub fn text_size(library: &Path) -> TextSize {
    ag_settings::get(library, TEXT_SIZE_KEY)
        .and_then(|value| value.as_str().map(TextSize::from_name))
        .unwrap_or_default()
}

/// Choose a text size.
pub fn save_text_size(library: &Path, size: TextSize) -> Result<(), String> {
    let value = serde_json::to_value(size).map_err(|e| e.to_string())?;
    ag_settings::set(library, TEXT_SIZE_KEY, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-editor-layout-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn panels_are_none_until_saved() {
        let dir = scratch("panels-default");
        assert_eq!(panels(&dir), None);
    }

    #[test]
    fn saved_panels_are_what_is_read_back() {
        let dir = scratch("panels-roundtrip");
        let left = LayoutPanels {
            navigator_open: false,
            tab: None,
            tab_width_px: 300,
            history_open: false,
            history_height_px: Some(180),
            navigator_floating: true,
            navigator_at: Some([0.6, 0.1]),
        };
        save_panels(&dir, &left).expect("write");
        assert_eq!(panels(&dir), Some(left));
    }

    /// Panels saved before History folded or the Navigator floated still read,
    /// as History open at its own height and the Navigator in the column.
    #[test]
    fn panels_saved_by_an_earlier_version_still_read() {
        let dir = scratch("panels-earlier");
        ag_settings::set(
            &dir,
            PANELS_KEY,
            serde_json::json!({ "navigatorOpen": true, "tab": null, "tabWidthPx": 245 }),
        )
        .expect("write");
        let read = panels(&dir).expect("readable");
        assert!(read.history_open);
        assert_eq!(read.history_height_px, None);
        assert!(!read.navigator_floating);
        assert_eq!(read.navigator_at, None);
    }

    #[test]
    fn a_value_that_is_not_panels_reads_as_none() {
        let dir = scratch("panels-foreign");
        ag_settings::set(
            &dir,
            PANELS_KEY,
            serde_json::json!({ "navigatorOpen": false }),
        )
        .expect("write");
        assert_eq!(panels(&dir), None);
        ag_settings::set(&dir, PANELS_KEY, Value::from("not an object")).expect("write");
        assert_eq!(panels(&dir), None);
    }

    #[test]
    fn grey_until_another_palette_is_chosen() {
        let dir = scratch("theme");
        assert_eq!(theme(&dir), ModernTheme::Grey);
        save_theme(&dir, ModernTheme::Light).expect("write");
        assert_eq!(theme(&dir), ModernTheme::Light);
        assert_eq!(ModernTheme::from_name("deep"), ModernTheme::Deep);
        assert_eq!(ModernTheme::from_name("dark"), ModernTheme::Grey);
        assert_eq!(
            serde_json::to_value(ModernTheme::Deep).expect("serialise"),
            Value::from("deep")
        );
    }

    #[test]
    fn normal_until_another_text_size_is_chosen() {
        let dir = scratch("text-size");
        assert_eq!(text_size(&dir), TextSize::Normal);
        save_text_size(&dir, TextSize::Small).expect("write");
        assert_eq!(text_size(&dir), TextSize::Small);
        assert_eq!(TextSize::from_name("large"), TextSize::Large);
        assert_eq!(TextSize::from_name("huge"), TextSize::Normal);
    }

    /// The window's preferences share a file with every other preference of
    /// ours; saving them must not disturb a neighbour.
    #[test]
    fn saving_leaves_the_other_preferences_alone() {
        let dir = scratch("neighbours");
        ag_settings::set(&dir, "compactSliderGap", Value::from(6)).expect("write");
        save_theme(&dir, ModernTheme::Deep).expect("write");
        save_text_size(&dir, TextSize::Large).expect("write");
        let left = LayoutPanels {
            navigator_open: true,
            tab: Some("history".to_string()),
            tab_width_px: 245,
            history_open: true,
            history_height_px: None,
            navigator_floating: false,
            navigator_at: None,
        };
        save_panels(&dir, &left).expect("write");
        assert_eq!(
            ag_settings::get(&dir, "compactSliderGap"),
            Some(Value::from(6))
        );
    }
}
