//! The contract every tool implements.
//!
//! A tool is a type with a stable id, the stage it runs in, the kind of work
//! it does, a settings struct, and one function from an image to an image.
//! That is all of it. The registry puts tools in order and the pipeline runs
//! and caches them; neither knows anything about a particular tool, which is
//! what lets a new one arrive as one file and one registry line.

use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Error, Gpu, GpuSlot, Image, Result, Source};

/// Where a tool's code comes from, read off the first three characters of
/// its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// `dt_`
    Darktable,
    /// `rt_`
    RawTherapee,
    /// `rr_`
    RapidRaw,
    /// `rm_`
    Rawmakase,
    /// `ag_` - Argentum's own.
    Argentum,
}

impl Origin {
    pub const ALL: [Origin; 5] = [
        Origin::Darktable,
        Origin::RawTherapee,
        Origin::RapidRaw,
        Origin::Rawmakase,
        Origin::Argentum,
    ];

    pub const fn prefix(self) -> &'static str {
        match self {
            Origin::Darktable => "dt_",
            Origin::RawTherapee => "rt_",
            Origin::RapidRaw => "rr_",
            Origin::Rawmakase => "rm_",
            Origin::Argentum => "ag_",
        }
    }

    /// The origin an id names, or `None` when it has no known prefix or is
    /// not a usable id: lowercase ASCII letters, digits and `_`, with a name
    /// after the prefix. Ids are keys in saved edits, so they stay that
    /// plain. `const` so that `Registry::with` can refuse a bad id at compile
    /// time.
    pub const fn of(id: &str) -> Option<Origin> {
        let b = id.as_bytes();
        if b.len() < 4 || b[2] != b'_' {
            return None;
        }
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_') {
                return None;
            }
            i += 1;
        }
        match (b[0], b[1]) {
            (b'd', b't') => Some(Origin::Darktable),
            (b'r', b't') => Some(Origin::RawTherapee),
            (b'r', b'r') => Some(Origin::RapidRaw),
            (b'r', b'm') => Some(Origin::Rawmakase),
            (b'a', b'g') => Some(Origin::Argentum),
            _ => None,
        }
    }
}

/// Where in the pipeline a tool runs.
///
/// Tools run in registry order, and the registry has to list them in stage
/// order: every `Raw` tool, then every `SceneLinear` one, then `Display`.
/// Between the last `Raw` tool and the first `SceneLinear` one the pipeline
/// demosaics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Sensor data before demosaic: `Image::Raw`.
    Raw,
    /// Demosaiced linear light, before anything is shaped for a screen:
    /// `Image::Rgb`.
    SceneLinear,
    /// Shaped for a screen or a file: `Image::Rgb`.
    Display,
}

/// The kind of work a tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Each output pixel from the same input pixel alone.
    PerPixel,
    /// Several passes over the whole image on the GPU. The only kind that
    /// `Context::gpu` hands a device to.
    GpuMultiPass,
    /// Whole-image work on the CPU.
    Cpu,
}

/// The allowed span of one numeric setting, by its name in the settings'
/// JSON. Values outside it are clamped whenever settings are read or set, so
/// a stale or hand-edited edit can never hand a tool something out of range.
/// An array of numbers is clamped element by element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub field: &'static str,
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub const fn new(field: &'static str, min: f64, max: f64) -> Self {
        Self { field, min, max }
    }
}

/// A tool's settings: what is saved in an edit and hashed into the cache key.
///
/// Give the struct `#[serde(default)]`, so a saved edit missing a field reads
/// that field's default rather than failing.
pub trait Settings:
    Serialize + DeserializeOwned + Default + Clone + PartialEq + Debug + Send + Sync + 'static
{
    /// Raised whenever a saved value would mean something different. Edits
    /// store it beside the settings, and older ones go through `migrate`.
    const VERSION: u32;

    /// The span of each numeric setting.
    const RANGES: &'static [Range] = &[];

    /// Turn settings saved at version `from`, older than `VERSION`, into this
    /// version's JSON. The default knows no older version.
    fn migrate(from: u32, value: Value) -> std::result::Result<Value, String> {
        let _ = value;
        Err(format!("no migration from version {from}"))
    }
}

/// One tool.
pub trait Tool: Default + Send + Sync + 'static {
    /// Stable for ever once an edit has saved it: origin prefix and a name,
    /// lowercase, as `Origin::of` describes.
    const ID: &'static str;
    const STAGE: Stage;
    const KIND: Kind;
    type Settings: Settings;

    /// `Ok(None)` means there is nothing to do: the input is the output, and
    /// the pipeline keeps the same picture rather than a copy of it.
    fn process(
        &self,
        settings: &Self::Settings,
        input: &Image,
        cx: &Context,
    ) -> Result<Option<Image>>;
}

/// What a tool can see besides its input.
pub struct Context<'a> {
    /// The photo as rawler decoded it, before any tool touched it - for a
    /// measurement that needs the sensor's own data, such as capture
    /// sharpening's radius.
    pub source: &'a Source,
    /// Names the input exactly: the same key is the same pixels. For a tool
    /// that keeps a measurement between runs.
    pub input_key: u64,
    tool: &'static str,
    kind: Kind,
    gpu: &'a GpuSlot,
}

impl<'a> Context<'a> {
    /// The context `T` runs in. The pipeline makes these; so can a caller
    /// running one tool on its own.
    pub fn new<T: Tool>(source: &'a Source, input_key: u64, gpu: &'a GpuSlot) -> Self {
        Self::for_tool(T::ID, T::KIND, source, input_key, gpu)
    }

    pub(crate) fn for_tool(
        tool: &'static str,
        kind: Kind,
        source: &'a Source,
        input_key: u64,
        gpu: &'a GpuSlot,
    ) -> Self {
        Self {
            source,
            input_key,
            tool,
            kind,
            gpu,
        }
    }

    /// The engine's GPU device, opened the first time any tool asks.
    pub fn gpu(&self) -> Result<Arc<Gpu>> {
        if self.kind != Kind::GpuMultiPass {
            return Err(Error::Tool {
                tool: self.tool,
                message: "only a GpuMultiPass tool is given the GPU".into(),
            });
        }
        self.gpu.get()
    }
}

/// Read settings saved at `version`: migrated if older, refused if newer,
/// then clamped to `RANGES`.
pub fn read_settings<S: Settings>(version: u32, value: Value) -> std::result::Result<S, String> {
    let value = match version.cmp(&S::VERSION) {
        std::cmp::Ordering::Equal => value,
        std::cmp::Ordering::Less => S::migrate(version, value)?,
        std::cmp::Ordering::Greater => {
            return Err(format!(
                "saved at version {version}, newer than this engine's {}",
                S::VERSION
            ));
        }
    };
    let (value, _) = clamp_json(value, S::RANGES);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The same settings with every value inside its range.
pub fn clamp<S: Settings>(settings: S) -> S {
    let Ok(value) = serde_json::to_value(&settings) else {
        return settings;
    };
    match clamp_json(value, S::RANGES) {
        (value, true) => serde_json::from_value(value).unwrap_or(settings),
        (_, false) => settings,
    }
}

/// Clamp the fields `ranges` names. Says whether anything moved, so settings
/// already in range are kept exactly as they were.
fn clamp_json(mut value: Value, ranges: &[Range]) -> (Value, bool) {
    let mut moved = false;
    if let Value::Object(map) = &mut value {
        for range in ranges {
            if let Some(v) = map.get_mut(range.field) {
                moved |= clamp_value(v, range);
            }
        }
    }
    (value, moved)
}

fn clamp_value(v: &mut Value, r: &Range) -> bool {
    match v {
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |moved, item| clamp_value(item, r) | moved),
        Value::Number(n) => {
            // Integers stay integers, or a u32 field would not read back.
            let next = if let Some(i) = n.as_i64() {
                let c = i.clamp(r.min.ceil() as i64, r.max.floor() as i64);
                (c != i).then(|| Value::from(c))
            } else if n.as_u64().is_some() {
                Some(Value::from(r.max.floor() as i64))
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                let c = f.clamp(r.min, r.max);
                (c != f).then(|| Value::from(c))
            };
            match next {
                Some(clamped) => {
                    *v = clamped;
                    true
                }
                None => false,
            }
        }
        _ => false,
    }
}

/// What a UI panel needs to know about a tool, without knowing its type.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInfo {
    pub id: &'static str,
    pub origin: Origin,
    pub stage: Stage,
    pub kind: Kind,
    pub version: u32,
    pub ranges: &'static [Range],
    pub defaults: Value,
}

// ---------------------------------------------------------------------------
// The same contract with the types erased, so one registry can hold them all.
// ---------------------------------------------------------------------------

pub(crate) trait AnySettings: Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn to_json(&self) -> Value;
}

impl<S: Settings> AnySettings for S {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub(crate) trait AnyTool: Send + Sync {
    fn info(&self) -> ToolInfo;
    fn defaults(&self) -> Box<dyn AnySettings>;
    fn read(&self, version: u32, value: Value)
    -> std::result::Result<Box<dyn AnySettings>, String>;
    fn process(
        &self,
        settings: &dyn AnySettings,
        input: &Image,
        cx: &Context,
    ) -> Result<Option<Image>>;
}

pub(crate) struct Registered<T: Tool>(pub T);

impl<T: Tool> AnyTool for Registered<T> {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            id: T::ID,
            origin: Origin::of(T::ID).expect("checked when registered"),
            stage: T::STAGE,
            kind: T::KIND,
            version: <T::Settings as Settings>::VERSION,
            ranges: <T::Settings as Settings>::RANGES,
            defaults: T::Settings::default().to_json(),
        }
    }

    fn defaults(&self) -> Box<dyn AnySettings> {
        Box::new(T::Settings::default())
    }

    fn read(
        &self,
        version: u32,
        value: Value,
    ) -> std::result::Result<Box<dyn AnySettings>, String> {
        read_settings::<T::Settings>(version, value).map(|s| Box::new(s) as Box<dyn AnySettings>)
    }

    fn process(
        &self,
        settings: &dyn AnySettings,
        input: &Image,
        cx: &Context,
    ) -> Result<Option<Image>> {
        let settings = settings
            .as_any()
            .downcast_ref::<T::Settings>()
            .expect("a tool's settings slot only ever holds its own settings");
        self.0.process(settings, input, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn origins_come_from_the_prefix() {
        assert_eq!(Origin::of("dt_highlights"), Some(Origin::Darktable));
        assert_eq!(Origin::of("rt_capture_sharpen"), Some(Origin::RawTherapee));
        assert_eq!(Origin::of("rr_tone"), Some(Origin::RapidRaw));
        assert_eq!(Origin::of("rm_grain"), Some(Origin::Rawmakase));
        assert_eq!(Origin::of("ag_exposure_wb"), Some(Origin::Argentum));
        for prefix in Origin::ALL.map(Origin::prefix) {
            assert_eq!(Origin::of(prefix), None, "a prefix alone is not an id");
        }
        assert_eq!(Origin::of("xx_tool"), None);
        assert_eq!(Origin::of("dt_Highlights"), None, "lowercase only");
        assert_eq!(Origin::of("dt_high-lights"), None);
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Demo {
        amount: f32,
        count: u32,
        gains: [f32; 2],
    }

    impl Default for Demo {
        fn default() -> Self {
            Self {
                amount: 0.5,
                count: 3,
                gains: [1.0, 1.0],
            }
        }
    }

    impl Settings for Demo {
        const VERSION: u32 = 2;
        const RANGES: &'static [Range] = &[
            Range::new("amount", 0.0, 1.0),
            Range::new("count", 1.0, 10.0),
            Range::new("gains", 0.25, 4.0),
        ];

        fn migrate(from: u32, mut value: Value) -> std::result::Result<Value, String> {
            if from != 1 {
                return Err(format!("no migration from version {from}"));
            }
            // Version 1 stored a percentage.
            if let Some(pct) = value.get("amount").and_then(Value::as_f64) {
                value["amount"] = Value::from(pct / 100.0);
            }
            Ok(value)
        }
    }

    #[test]
    fn values_out_of_range_are_clamped_and_types_kept() {
        let s: Demo = read_settings(
            2,
            serde_json::json!({ "amount": 7.0, "count": 0, "gains": [9.0, 0.1] }),
        )
        .unwrap();
        assert_eq!(
            s,
            Demo {
                amount: 1.0,
                count: 1,
                gains: [4.0, 0.25]
            }
        );
    }

    #[test]
    fn missing_fields_read_as_defaults() {
        let s: Demo = read_settings(2, serde_json::json!({ "count": 5 })).unwrap();
        assert_eq!(
            s,
            Demo {
                count: 5,
                ..Demo::default()
            }
        );
    }

    #[test]
    fn older_versions_migrate_and_newer_ones_are_refused() {
        let s: Demo = read_settings(1, serde_json::json!({ "amount": 25.0 })).unwrap();
        assert_eq!(s.amount, 0.25);
        assert!(read_settings::<Demo>(3, serde_json::json!({})).is_err());
        assert!(read_settings::<Demo>(0, serde_json::json!({})).is_err());
    }

    #[test]
    fn settings_in_range_are_kept_bit_for_bit() {
        let s = Demo {
            amount: 0.1 + 0.2,
            count: 4,
            gains: [1.0 / 3.0, 2.0],
        };
        assert_eq!(clamp(s.clone()), s);
        assert_eq!(
            clamp(Demo {
                amount: -1.0,
                ..s.clone()
            })
            .amount,
            0.0
        );
    }
}
