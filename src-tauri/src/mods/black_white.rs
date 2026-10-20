//! B&W: Lightroom's Treatment, Black & White, with its eight-band mix. Ours,
//! after RAWmakase.
//!
//! An edit's `agBlackWhite` is `{ enabled, reds, oranges, yellows, greens,
//! aquas, blues, purples, magentas }`, each band -100..100 as in Lightroom's
//! B&W Mix: how light that colour turns in the grey. Switched on, the photo
//! turns grey in `ag_stage_color` (modules.wgsl), after Saturation and before
//! Color Grading, so grading tints the grey as Lightroom's does: that is how
//! the Sepia preset is made. Their grading is gentler than Lightroom's split
//! toning, so Sepia's tint shows mostly in the light tones.
//!
//! The grey is RAWmakase's measured mix, in the engine
//! (`argentum_engine::rm_black_white`): `gray_grid` turns a mix into one
//! number per cell of a hue x saturation x value grid, which a render that
//! asks for B&W writes to the GPU (gpu_processing.rs), and the shader looks
//! each pixel up in it (`rm_gray` in modules.wgsl, the engine's `gray` on
//! the GPU). Here: the edit's settings, what B&W leaves out, and what a
//! Lightroom preset's black & white settings become.
//!
//! COPIED FROM RAWmakase @ a8c5d6d7 (v0.4.0), MIT
//!
//! - The lookup in modules.wgsl: crates/rawmakase-engine/src/develop/gpu/develop.wgsl,
//!   `mixer` with only its value change, and `adjust`'s black & white step.
//! - The preset keys, `ConvertToGrayscale` and `GrayMixer<Band>`:
//!   crates/rawmakase-interop/src/xmp/apply.rs.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - Color Grading tints the grey. RAWmakase grades before its grey step
//!   (`color_stage` in pipeline/pixel.rs), so its Sepia renders untinted.
//! - The grey is taken from the scene-linear colour after Saturation, where
//!   the app's colour controls are, and before its tone curve; RAWmakase takes
//!   it after its tone curve. The chart's value axis is read on the same scale.
//! - While B&W is on, the Color Mixer, Saturation, Vibrance and Hue are left
//!   out of the render and kept in the edit, as Lightroom greys them out.
//! - No Auto mix yet: a mix starts at zero.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::image_processing::{GlobalAdjustments, HslColor};

pub use argentum_engine::rm_black_white::{CELLS, gray_grid};

/// The key in an edit.
pub const KEY: &str = "agBlackWhite";

/// The bands as the edit names them, in Lightroom's and RAWmakase's order.
pub const BANDS: [&str; 8] = [
    "reds", "oranges", "yellows", "greens", "aquas", "blues", "purples", "magentas",
];

/// The same bands as a Lightroom preset names them (`GrayMixer<Band>`).
const XMP_BANDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];

/// B&W's settings as the shader reads them (modules.wgsl `AgBlackWhite`), the
/// last field of their `GlobalAdjustments`. 48 bytes, a multiple of 16.
#[repr(C)]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default, PartialEq)]
pub struct Params {
    /// 1 turns the photo grey.
    pub enabled: u32,
    _pad: [u32; 3],
    /// Each band, -1..1, in `BANDS` order.
    pub mix: [f32; 8],
}

impl Params {
    /// An edit's B&W. Off while the Color section's eye is closed
    /// (`visible`), as its other settings are.
    pub fn from_json(adjustments: &Value, visible: bool) -> Self {
        let settings = &adjustments[KEY];
        if !visible || settings["enabled"].as_bool() != Some(true) {
            return Self::default();
        }
        Self {
            enabled: 1,
            _pad: [0; 3],
            mix: BANDS.map(|band| {
                (settings[band].as_f64().unwrap_or(0.0) as f32 / 100.0).clamp(-1.0, 1.0)
            }),
        }
    }

    pub fn is_on(&self) -> bool {
        self.enabled != 0
    }
}

/// While B&W is on, the colour controls Lightroom greys out leave the render:
/// the Color Mixer, Saturation, Vibrance and Hue, which would otherwise move
/// colours before the mix reads them. The edit keeps them for when the photo
/// goes back to Color; masks keep their own.
pub fn leave_out_colour_controls(global: &mut GlobalAdjustments) {
    if !global.ag_black_white.is_on() {
        return;
    }
    global.saturation = 0.0;
    global.vibrance = 0.0;
    global.hue = 0.0;
    global.hsl = [HslColor::default(); 8];
}

/// A Lightroom preset's black & white settings as an edit's `agBlackWhite`,
/// when it has any: `ConvertToGrayscale` and the `GrayMixer` bands it names.
/// `None` for a preset that leaves the treatment alone (the flag absent or
/// empty, as Lightroom writes it in colour presets).
pub fn from_xmp(xmp: &str) -> Option<Value> {
    let attribute = |name: &str| -> Option<String> {
        let start = xmp.find(&format!("crs:{name}=\""))? + name.len() + 6;
        let end = xmp[start..].find('"')? + start;
        Some(xmp[start..end].to_string())
    };
    let enabled = match attribute("ConvertToGrayscale")?.as_str() {
        "True" => true,
        "False" => false,
        _ => return None,
    };
    let mut settings = Map::new();
    settings.insert("enabled".into(), Value::from(enabled));
    for (band, xmp_band) in BANDS.iter().zip(XMP_BANDS) {
        if let Some(v) = attribute(&format!("GrayMixer{xmp_band}"))
            .and_then(|v| v.trim_start_matches('+').parse::<f64>().ok())
        {
            settings.insert((*band).into(), Value::from(v.clamp(-100.0, 100.0)));
        }
    }
    Some(Value::Object(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_shader_reads_48_bytes() {
        assert_eq!(std::mem::size_of::<Params>(), 48);
    }

    #[test]
    fn an_edit_turns_it_on_with_its_mix() {
        let edit = json!({ KEY: { "enabled": true, "reds": 40, "blues": -150 } });
        let p = Params::from_json(&edit, true);
        assert!(p.is_on());
        assert_eq!(p.mix, [0.4, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
        // The Color section's eye closed, or switched off, or never set: colour.
        assert_eq!(Params::from_json(&edit, false), Params::default());
        assert!(
            !Params::from_json(&json!({ KEY: { "enabled": false, "reds": 40 } }), true).is_on()
        );
        assert!(!Params::from_json(&json!({}), true).is_on());
    }

    #[test]
    fn b_and_w_leaves_the_colour_controls_out() {
        let mut global = GlobalAdjustments::default();
        global.saturation = 0.3;
        global.vibrance = 0.2;
        global.hue = 10.0;
        leave_out_colour_controls(&mut global);
        assert_eq!(
            (global.saturation, global.vibrance, global.hue),
            (0.3, 0.2, 10.0)
        );
        global.ag_black_white = Params::from_json(&json!({ KEY: { "enabled": true } }), true);
        leave_out_colour_controls(&mut global);
        assert_eq!(
            (global.saturation, global.vibrance, global.hue),
            (0.0, 0.0, 0.0)
        );
        assert!(bytemuck::bytes_of(&global.hsl).iter().all(|&b| b == 0));
    }

    #[test]
    fn a_preset_names_its_treatment_and_mix() {
        let red = r#"crs:ConvertToGrayscale="True"
   crs:GrayMixerRed="40"
   crs:GrayMixerOrange="+30"
   crs:GrayMixerBlue="-50""#;
        assert_eq!(
            from_xmp(red),
            Some(json!({ "enabled": true, "reds": 40.0, "oranges": 30.0, "blues": -50.0 }))
        );
        assert_eq!(
            from_xmp(r#"crs:ConvertToGrayscale="False""#),
            Some(json!({ "enabled": false }))
        );
        // Lightroom writes the flag empty in colour presets: nothing changes.
        assert_eq!(from_xmp(r#"crs:ConvertToGrayscale="""#), None);
        assert_eq!(from_xmp(r#"crs:Saturation="10""#), None);
    }
}

/// Through the real shader, on a GPU: `cargo test --lib black_white::gpu -- --ignored`.
#[cfg(test)]
mod gpu {
    use super::*;
    use crate::mods::dng_standard::gpu::{centre, device, image_of, render};
    use crate::white_balance::WhiteBalance;
    use argentum_engine::rm_black_white::gray;
    use serde_json::json;

    /// Greys, and colours round the hue circle at three levels.
    fn colours() -> Vec<[f32; 3]> {
        let mut out: Vec<[f32; 3]> = [0.02, 0.18, 0.6].iter().map(|&v| [v; 3]).collect();
        for level in [0.05, 0.2, 0.6] {
            for c in [
                [1.0, 0.0, 0.0],
                [1.0, 0.45, 0.05],
                [0.9, 0.85, 0.1],
                [0.1, 0.8, 0.15],
                [0.1, 0.7, 0.8],
                [0.1, 0.35, 1.0],
                [0.75, 0.2, 0.8],
                [0.9, 0.2, 0.5],
                [0.8, 0.55, 0.4],
                [0.5, 0.6, 0.7],
            ] {
                out.push(c.map(|v: f32| v * level));
            }
        }
        out
    }

    fn edit(black_white: Value, extra: Value) -> Value {
        let mut js = json!({ "agSharpen": { "capture": false, "amount": 0 }, KEY: black_white });
        if let (Some(js), Some(extra)) = (js.as_object_mut(), extra.as_object()) {
            js.extend(extra.clone());
        }
        js
    }

    fn settings(mix: [f32; 8]) -> Value {
        let mut s = Map::new();
        s.insert("enabled".into(), Value::from(true));
        for (band, v) in BANDS.iter().zip(mix) {
            s.insert((*band).into(), Value::from(f64::from(v) * 100.0));
        }
        Value::Object(s)
    }

    /// The shader's grey is the chart's: a colour rendered in B&W comes out as
    /// the grey `gray` gives it, rendered in Color. Up to that point the
    /// render changes nothing on these photos, so both meet the same stages
    /// after it, tone curve included.
    #[test]
    #[ignore = "needs a GPU; run with --ignored"]
    fn the_shader_turns_each_colour_into_the_grey_the_chart_gives() {
        let ctx = device().expect("no wgpu adapter");
        let wb = WhiteBalance::reference();
        let colours = colours();
        for mix in [
            [0.0; 8],
            [0.4, 0.3, 0.1, -0.1, -0.3, -0.5, 0.0, 0.0],
            [-0.2, -0.1, 0.2, 0.4, 0.0, -0.1, 0.0, 0.0],
            [1.0, -1.0, 0.75, -0.75, 0.5, -0.5, 0.25, -0.25],
        ] {
            let got = render(
                &ctx,
                &image_of(&colours),
                edit(settings(mix), json!({})),
                wb,
            );
            let grid = gray_grid(mix);
            let greys: Vec<[f32; 3]> = colours.iter().map(|&c| [gray(&grid, c); 3]).collect();
            let want = render(
                &ctx,
                &image_of(&greys),
                edit(json!({ "enabled": false }), json!({})),
                wb,
            );
            let mut worst = 0.0f32;
            for (i, c) in colours.iter().enumerate() {
                let (g, w) = (centre(&got, i), centre(&want, i));
                assert!(
                    (g[0] - g[1]).abs() < 1e-5 && (g[1] - g[2]).abs() < 1e-5,
                    "{mix:?} {c:?}: not grey, {g:?}"
                );
                worst = worst.max((g[1] - w[1]).abs() * 255.0);
            }
            eprintln!("{mix:?}: worst {worst:.3}/255 from the chart's grey");
            assert!(
                worst < 0.5,
                "{mix:?}: the shader is {worst:.3}/255 from the chart's grey"
            );
        }
    }

    /// Color Grading runs after the grey and tints it, as Sepia needs; with
    /// B&W off, Saturation and the Color Mixer work as before.
    #[test]
    #[ignore = "needs a GPU; run with --ignored"]
    fn grading_tints_the_grey_and_colour_controls_wait() {
        let ctx = device().expect("no wgpu adapter");
        let wb = WhiteBalance::reference();
        let image = image_of(&colours());
        let sepia = json!({ "colorGrading": {
            "shadows": { "hue": 38, "saturation": 25 },
            "highlights": { "hue": 45, "saturation": 18 },
            "blending": 50,
        }});
        let toned = render(&ctx, &image, edit(settings([0.0; 8]), sepia), wb);
        // Their grading tints highlights most: a light grey, warmed.
        let light = centre(&toned, 2);
        assert!(
            light[0] > light[2] + 0.005,
            "Sepia leaves the grey untinted: {light:?}"
        );
        // Saturation and the Color Mixer, kept in the edit, do nothing in B&W.
        let colour_controls = json!({ "saturation": 60, "hsl": { "reds": { "luminance": -80 } } });
        let plain = render(&ctx, &image, edit(settings([0.0; 8]), json!({})), wb);
        let with = render(
            &ctx,
            &image,
            edit(settings([0.0; 8]), colour_controls.clone()),
            wb,
        );
        assert_eq!(plain.as_raw(), with.as_raw());
        let off = |extra| render(&ctx, &image, edit(json!({ "enabled": false }), extra), wb);
        assert_ne!(off(json!({})).as_raw(), off(colour_controls).as_raw());
    }
}

/// Renders for judging B&W by eye, on real photos: `AG_RAWS` (paths separated
/// by ';') and `AG_OUT` set, `cargo test --lib black_white::sheet -- --ignored`.
#[cfg(test)]
mod sheet {
    use crate::mods::dng_standard::gpu::device;
    use image::{DynamicImage, GenericImageView};
    use serde_json::Value;

    fn save(image: &DynamicImage, path: &std::path::Path) {
        let file = std::fs::File::create(path).expect("create");
        let mut jpeg = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 95);
        jpeg.encode_image(&DynamicImage::ImageRgb8(image.to_rgb8()))
            .expect("write jpeg");
    }

    /// Each RAW developed as the app does and rendered as it opens (Color),
    /// then with each B&W preset that comes with the app on that same start:
    /// `<n>-<look>.jpg`, `AG_WIDTH` wide (1200 by default), and, where
    /// `AG_CROPS` gives the photo a part `x,y,w,h` (fractions, ';' per RAW),
    /// `<n>-<look>-crop.jpg` of that part at full size.
    #[test]
    #[ignore = "reads RAW files; run by hand"]
    fn sheet_renders() {
        let raws = std::env::var("AG_RAWS").expect("set AG_RAWS");
        let out = std::path::PathBuf::from(std::env::var("AG_OUT").expect("set AG_OUT"));
        let width: u32 = std::env::var("AG_WIDTH")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(1200);
        let crops: Vec<Option<[f32; 4]>> = std::env::var("AG_CROPS")
            .unwrap_or_default()
            .split(';')
            .map(|c| {
                let v: Vec<f32> = c.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
            })
            .collect();
        std::fs::create_dir_all(&out).unwrap();
        let ctx = device().expect("no wgpu adapter");
        let shipped =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/presets/rawmakase");
        let mut looks: Vec<(String, Value)> = vec![("Color".into(), Value::Null)];
        for group in crate::mods::looks::builtin_presets(&shipped) {
            if group.name == "B&W" {
                looks.extend(group.presets.into_iter().map(|p| (p.name, p.adjustments)));
            }
        }
        for (n, path) in raws.split(';').filter(|p| !p.trim().is_empty()).enumerate() {
            let bytes = std::fs::read(path).expect("read raw");
            let as_shot = crate::raw_processing::read_as_shot(&bytes)
                .unwrap_or_else(crate::as_shot::AsShot::reference);
            let linear = crate::raw_processing::develop_raw_image(
                &bytes,
                false,
                2.5,
                "default".to_string(),
                None,
                Some(path),
            )
            .expect("decodes");
            let crop = crops.get(n).copied().flatten();
            let source = if crop.is_some() {
                linear
            } else {
                let h = (linear.height() as f32 * width as f32 / linear.width() as f32) as u32;
                crate::image_processing::downscale_f32_image(&linear, width, h)
            };
            for (look, js) in &looks {
                let adjustments = crate::image_processing::get_all_adjustments_from_json(
                    js,
                    true,
                    as_shot,
                    Default::default(),
                    Some(path),
                );
                let rendered = crate::mods::export_precision::render_high_precision(
                    &ctx,
                    &source,
                    crate::gpu_processing::RenderRequest {
                        adjustments,
                        mask_bitmaps: &[],
                        lut: None,
                        roi: None,
                    },
                )
                .expect("renders");
                let name = format!("{}-{}", n + 1, look.to_lowercase().replace(' ', "-"));
                let (w, h) = rendered.dimensions();
                let whole =
                    rendered.resize(width, width * h / w, image::imageops::FilterType::Lanczos3);
                save(&whole, &out.join(format!("{name}.jpg")));
                if let Some([x, y, cw, ch]) = crop {
                    let part = rendered.crop_imm(
                        (x * w as f32) as u32,
                        (y * h as f32) as u32,
                        (cw * w as f32) as u32,
                        (ch * h as f32) as u32,
                    );
                    save(&part, &out.join(format!("{name}-crop.jpg")));
                }
                eprintln!("{path}: {look}");
            }
        }
    }
}
