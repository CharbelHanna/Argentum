//! The camera's built-in lens correction, in today's app. Ours.
//!
//! The tables are read from the RAW by the engine (`rm_lens_builtin.rs`,
//! from RAWmakase) and travel in the photo's edit the way lensfun's numbers
//! do (`lensDistortionParams`), so the preview, the export, the masks and the
//! thumbnail all see the same correction with no path of their own:
//!
//! | Key | What |
//! |---|---|
//! | `lensBuiltin` | the switch in Crop > Lens Correction; absent until the photo is opened |
//! | `lensBuiltinParams` | the photo's own tables, as `rm_lens_builtin::LensCorrection` |
//!
//! `argentum/lensBuiltin.ts` fills both in when a photo opens, from
//! `lens_builtin` below. While the switch is on, the tables replace the
//! lensfun profile (`get_geometry_params_from_json` in `image_processing.rs`),
//! so nothing is corrected twice, and the panel's Distortion, Chromatic
//! Aberration and Vignetting switches and amounts act on them.
//!
//! WHICH PHOTOS START WITH IT ON
//!
//! A correction Lightroom applies by default (`default_on`: Fujifilm's) is on
//! for a photo with no edit yet. A photo edited before this existed keeps the
//! look it was edited with until the switch is turned on. Sony's is not known
//! to be applied by Lightroom, so it starts off: the switch is there, off.
//! A DNG's opcodes are what Lightroom applies, so they start on, as
//! Fujifilm's do.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use argentum_engine::rm_lens_builtin::{Amounts, LensCorrection, LensMap};
use serde::Serialize;

/// The edit's keys for it, which the preview's geometry caches must see.
pub const KEYS: [&str; 2] = ["lensBuiltin", "lensBuiltinParams"];

/// The correction one render applies, and how much of each part.
#[derive(Clone, Debug)]
pub struct Builtin {
    pub correction: Arc<LensCorrection>,
    pub amounts: Amounts,
}

impl Builtin {
    /// The correction the edit turns on, or `None` when it is off, the photo
    /// has none, or its tables do not read back.
    pub fn from_adjustments(adjustments: &serde_json::Value) -> Option<Builtin> {
        if adjustments.get(KEYS[0])?.as_bool() != Some(true) {
            return None;
        }
        let correction: LensCorrection =
            serde_json::from_value(adjustments.get(KEYS[1])?.clone()).ok()?;
        if correction.is_empty() || !correction.validate() {
            return None;
        }
        // A part switched off is an amount of zero; the sliders run 0 to 200.
        let amount = |enabled: &str, amount: &str| {
            if adjustments[enabled].as_bool().unwrap_or(true) {
                adjustments[amount].as_f64().unwrap_or(100.0) as f32 / 100.0
            } else {
                0.0
            }
        };
        Some(Builtin {
            correction: Arc::new(correction),
            amounts: Amounts {
                distortion: amount("lensDistortionEnabled", "lensDistortionAmount"),
                chromatic: amount("lensTcaEnabled", "lensTcaAmount"),
                vignetting: amount("lensVignetteEnabled", "lensVignetteAmount"),
            },
        })
    }

    /// The correction over an image of `width` by `height` pixels.
    pub fn map(&self, width: f32, height: f32) -> LensMap<'_> {
        LensMap::new(&self.correction, width, height, self.amounts)
    }
}

/// A geometry cache's `hash` with the edit's keys for it folded in. Their
/// `GEOMETRY_KEYS` (cache_utils.rs) has no room for ours, so the preview's
/// caches in lib.rs fold them in here.
pub fn fold(hash: u64, adjustments: &serde_json::Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash.hash(&mut hasher);
    hash_into(adjustments, &mut hasher);
    hasher.finish()
}

fn hash_into(adjustments: &serde_json::Value, hasher: &mut impl Hasher) {
    for key in KEYS {
        if let Some(value) = adjustments.get(key) {
            key.hash(hasher);
            value.to_string().hash(hasher);
        }
    }
}

/// What the editor needs when a photo opens.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// The photo's own tables; `None` when it carries none.
    pub correction: Option<LensCorrection>,
    /// The switch's starting position, for an edit that has none yet.
    pub default_on: bool,
    /// The photo has no saved edit, so My Lenses' "Correct new photos"
    /// decides how it opens (argentum/newPhotoDefaults.ts).
    pub fresh: bool,
}

/// The `ag` command `lens_builtin`: the photo's correction, read from its
/// file, and whether the switch starts on.
pub fn lens_builtin(path: &str) -> Result<Found, String> {
    let (source, sidecar) = crate::file_management::parse_virtual_path(path);
    let correction = {
        let bytes = crate::file_management::read_file_mapped(&source).map_err(|e| e.to_string())?;
        argentum_engine::rm_lens_builtin::read(&bytes)
    };
    let saved = crate::exif_processing::load_sidecar(&sidecar).adjustments;
    let edit = saved
        .as_object()
        .filter(|edit| !edit.is_empty() && edit.get("is_null").is_none());
    let edited_before = edit.is_some_and(|edit| !edit.contains_key(KEYS[0]));
    let default_on = correction.as_ref().is_some_and(|c| c.default_on) && !edited_before;
    Ok(Found {
        correction,
        default_on,
        fresh: edit.is_none(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_engine::rm_lens_builtin::Radial;
    use serde_json::json;

    fn correction() -> serde_json::Value {
        serde_json::to_value(LensCorrection {
            source: "Fujifilm built-in".into(),
            default_on: true,
            vignetting: Some(Radial {
                knots: vec![0., 1.],
                values: vec![1., 1.5],
            }),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn only_a_switched_on_edit_with_tables_applies_it() {
        let on = json!({ "lensBuiltin": true, "lensBuiltinParams": correction() });
        assert!(Builtin::from_adjustments(&on).is_some());
        let off = json!({ "lensBuiltin": false, "lensBuiltinParams": correction() });
        assert!(Builtin::from_adjustments(&off).is_none());
        assert!(Builtin::from_adjustments(&json!({ "lensBuiltin": true })).is_none());
        assert!(Builtin::from_adjustments(&json!({})).is_none());
        let garbage = json!({ "lensBuiltin": true, "lensBuiltinParams": { "knots": 3 } });
        assert!(Builtin::from_adjustments(&garbage).is_none());
    }

    #[test]
    fn the_panels_switches_and_amounts_set_how_much() {
        let edit = json!({
            "lensBuiltin": true,
            "lensBuiltinParams": correction(),
            "lensDistortionAmount": 150,
            "lensTcaEnabled": false,
            "lensVignetteAmount": 50,
        });
        let b = Builtin::from_adjustments(&edit).unwrap();
        assert_eq!(
            b.amounts,
            Amounts {
                distortion: 1.5,
                chromatic: 0.,
                vignetting: 0.5,
            }
        );
    }

    /// An image's values, each to five significant digits, as one FNV-1a
    /// hash: exact enough to catch any change to a warp, loose enough to
    /// ignore a platform's last-bit difference in the maths.
    fn digest(image: &image::DynamicImage) -> u64 {
        let text: String = (image.to_rgb32f().iter())
            .map(|v| format!("{v:.4e},"))
            .collect();
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        })
    }

    /// Today's warp and its inverse, pinned on a small gradient, with the
    /// camera's tables (Fujifilm's fill and Sony's), with lensfun's numbers,
    /// and with both, where the camera's replace lensfun's. A change meant to
    /// move one updates its line here; any other change fails.
    #[test]
    fn todays_warps_are_pinned() {
        use crate::image_processing::{
            get_geometry_params_from_json, unwarp_image_geometry, warp_image_geometry,
        };
        let image =
            image::DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(48, 32, |x, y| {
                image::Rgb([x as f32 / 48., y as f32 / 32., (x + y) as f32 / 80.])
            }));
        let radial = |values: Vec<f32>| Radial {
            knots: vec![0., 0.5, 1.],
            values,
        };
        let tables = |source: &str| {
            serde_json::to_value(LensCorrection {
                source: source.into(),
                default_on: true,
                vignetting: Some(radial(vec![1., 1.2, 1.6])),
                // A ZV-1-like table: 1.077 at the centre, so the fills differ.
                distortion: Some(radial(vec![1.077, 1.04, 0.93])),
                chromatic: Some([radial(vec![1., 1., 1.0006]), radial(vec![1., 1., 0.9995])]),
            })
            .unwrap()
        };
        let lensfun = json!({
            "k1": -0.02, "k2": 0.004, "k3": 0.0, "model": 0,
            "tca_vr": 1.0004, "tca_vb": 0.9997, "vig_k1": -0.3, "vig_k2": 0.05, "vig_k3": 0.0,
        });
        let pinned = |edit: serde_json::Value| {
            let params = get_geometry_params_from_json(&edit);
            let warped = warp_image_geometry(&image, params.clone());
            [
                digest(&warped),
                digest(&unwarp_image_geometry(&warped, params)),
            ]
        };
        let fuji = json!({ "lensBuiltin": true, "lensBuiltinParams": tables("Fujifilm built-in") });
        for (name, edit, want) in [
            (
                "camera",
                fuji.clone(),
                [8_378_816_467_656_231_270, 8_125_105_830_291_545_259],
            ),
            (
                "sony",
                json!({ "lensBuiltin": true, "lensBuiltinParams": tables(LensCorrection::SONY_BUILTIN) }),
                [6_215_859_517_951_140_240, 12_902_846_181_271_203_747],
            ),
            (
                "lensfun",
                json!({ "lensDistortionParams": lensfun }),
                [9_956_633_457_548_040_988, 7_218_553_381_868_940_216],
            ),
        ] {
            assert_eq!(pinned(edit), want, "{name}");
        }
        let mut both = fuji.clone();
        both["lensDistortionParams"] = lensfun;
        assert_eq!(
            pinned(both),
            pinned(fuji),
            "the camera's tables replace lensfun's"
        );
    }

    /// A photo is fresh while its sidecar holds no edit: no sidecar, an
    /// empty edit, or a reset one. Any saved value makes it edited.
    #[test]
    fn a_photo_without_a_saved_edit_is_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("a.jpg");
        std::fs::write(&photo, b"not a raw").unwrap();
        let path = photo.to_string_lossy().into_owned();
        let sidecar = crate::file_management::parse_virtual_path(&path).1;
        let fresh = |adjustments: Option<serde_json::Value>| {
            match adjustments {
                Some(a) => std::fs::write(
                    &sidecar,
                    json!({ "version": 1, "rating": 0, "adjustments": a }).to_string(),
                )
                .unwrap(),
                None => drop(std::fs::remove_file(&sidecar)),
            }
            lens_builtin(&path).unwrap().fresh
        };
        assert!(fresh(None));
        assert!(fresh(Some(json!({}))));
        assert!(fresh(Some(json!({ "is_null": true }))));
        assert!(!fresh(Some(json!({ "exposure": 0.5 }))));
        assert!(!fresh(Some(json!({ "lensBuiltin": false }))));
    }

    #[test]
    fn its_keys_change_the_geometry_hash() {
        let on = json!({ "lensBuiltin": true, "lensBuiltinParams": correction() });
        let off = json!({ "lensBuiltin": false, "lensBuiltinParams": correction() });
        assert_ne!(fold(7, &on), fold(7, &off));
        assert_eq!(fold(7, &on), fold(7, &on.clone()));
        assert_ne!(fold(7, &on), fold(8, &on));
    }
}
