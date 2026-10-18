//! The contract between the engine and the editor's interface.
//!
//! Every type the interface sees is here and nowhere else.
//! `src/shell/contract.generated.ts` is generated from this file, and
//! `src/shell/contract.fixtures.json` holds an example of each - both by
//! `tests/contract.rs`, which fails when either is stale. Regenerate with
//! `AG_WRITE_CONTRACT=1 cargo test --test contract`.
//!
//! A setting's value is JSON: a number, a boolean, or a string for a choice.
//! The TypeScript side calls it `SettingValue`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// The version of `EditDocument`'s own shape. Each tool's settings carry
/// their own version, in `ToolEdit::v`.
pub const EDIT_VERSION: u32 = 1;

/// Whose code a tool is: the first two letters of its id.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    #[serde(rename = "dt")]
    Darktable,
    #[serde(rename = "rt")]
    RawTherapee,
    #[serde(rename = "rr")]
    RapidRaw,
    #[serde(rename = "rm")]
    Rawmakase,
    #[serde(rename = "ag")]
    Argentum,
}

/// Where in the pipeline a tool runs: on the sensor data before demosaic,
/// on linear light, or on the picture shaped for a screen.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "scene")]
    SceneLinear,
    #[serde(rename = "display")]
    Display,
}

/// The panel a tool is shown in. More can be added.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Basic,
    Tone,
    Color,
    Detail,
    Lens,
    Effects,
}

/// What kind of control a setting is.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SettingKind {
    Float,
    Int,
    Bool,
    Choice,
}

#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
pub struct Choice {
    pub value: String,
    /// An i18n key.
    pub label: String,
}

/// One setting of a tool, as a panel shows it.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
pub struct SettingSpec {
    /// Its name in the tool's settings.
    pub key: String,
    /// An i18n key: `tools.<tool id>.<key>`.
    pub label: String,
    pub kind: SettingKind,
    #[ts(type = "SettingValue")]
    pub default: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub choices: Option<Vec<Choice>>,
    /// Shown under "More" rather than in the panel's first view.
    pub advanced: bool,
}

/// One tool, as the interface builds its panel.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
pub struct ToolDescriptor {
    /// Stable for ever: origin prefix and a name, e.g. `rt_capture_sharpen`.
    pub id: String,
    pub origin: Origin,
    /// An i18n key: `tools.<tool id>.label`.
    pub label: String,
    pub section: Section,
    /// The tool's place in the pipeline, and in its section.
    pub order: u32,
    pub stage: Stage,
    /// The version of its settings.
    pub version: u32,
    pub settings: Vec<SettingSpec>,
}

/// One tool's settings in an edit.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
pub struct ToolEdit {
    /// The version of the tool's settings these were written at.
    pub v: u32,
    #[ts(type = "Record<string, SettingValue>")]
    pub settings: BTreeMap<String, Value>,
}

/// Every tool's settings for one photo.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
pub struct EditDocument {
    /// `EDIT_VERSION`.
    pub version: u32,
    #[ts(type = "Record<string, ToolEdit>")]
    pub tools: BTreeMap<String, ToolEdit>,
}

/// A change to an edit: `{ toolId: { key: value } }`, merged into it. `null`
/// puts a setting back to its default. Only the tools it names, and the
/// tools after them, run again.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq, Default)]
pub struct EditPatch(
    #[ts(type = "Record<string, Record<string, SettingValue | null>>")]
    pub  BTreeMap<String, BTreeMap<String, Value>>,
);

/// Names an open photo.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoId(pub u32);

/// Names one requested render. A newer ticket cancels every older one.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenderTicket(pub u32);

/// What `open` returns.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub photo_id: PhotoId,
    /// The developed picture's size, in pixels.
    pub width: u32,
    pub height: u32,
    pub edit: EditDocument,
}

/// The part of the photo on screen.
#[derive(Serialize, Deserialize, TS, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// Screen pixels per photo pixel: 1 is 100%.
    pub zoom: f32,
    /// The point of the photo at the centre of the view, 0..1 across and
    /// down.
    pub center_x: f32,
    pub center_y: f32,
    /// The view's size in screen pixels.
    pub width: u32,
    pub height: u32,
}

/// What the engine tells the interface, as it happens.
#[derive(Serialize, Deserialize, TS, Debug, Clone, PartialEq)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum Event {
    /// A render is ready: `width` x `height` sRGB pixels, the view's part of
    /// the photo.
    Frame {
        ticket: RenderTicket,
        width: u32,
        height: u32,
    },
    /// 256 bins of luminance, over the frame with the same ticket.
    Histogram {
        ticket: RenderTicket,
        bins: Vec<u32>,
    },
    /// A render failed. Its ticket, when it was one.
    Error {
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<RenderTicket>,
        message: String,
    },
}

impl EditDocument {
    pub fn new() -> Self {
        Self {
            version: EDIT_VERSION,
            tools: BTreeMap::new(),
        }
    }
}

impl Default for EditDocument {
    fn default() -> Self {
        Self::new()
    }
}
