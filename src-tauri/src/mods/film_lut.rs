//! What a film look that comes with the app is fed. Ours.
//!
//! THE LUTS
//!
//! The film looks (`resources/film_luts`) are spektrafilm LUTs, which RapidRAW
//! ships and applies in place of the rendering: their scene-referred path in
//! shader.wgsl encodes the scene-linear picture with the V-Log curve and looks
//! it up. Each file names its input in its header, "Panasonic V-Log". In
//! spektrafilm's LUT creator that is the V-Log curve over V-Gamut primaries
//! (`spektrafilm_lut_creator/color_spaces.py` @ 3bb2c2d2, the 0.3.4 the LUTs
//! were made with: `"Panasonic V-Log", "V-Gamut", "V-Log"`), the code values
//! decoded and handed to the film as V-Gamut RGB.
//!
//! WHAT RAPIDRAW FEEDS THEM
//!
//! The curve over Argentum's working RGB, which is linear sRGB. V-Gamut's
//! primaries lie far outside sRGB's, so every colour reaches the film as a
//! stronger one: 1.4 to 1.6 times the chroma (OkLab) for skin, sky and
//! foliage, skin turned 11 degrees toward red and blues 15 toward cyan.
//!
//! WHAT THIS FEEDS THEM
//!
//! What the file declares: the picture converted to V-Gamut before the curve
//! (`ag_film_lut_input` in modules.wgsl). Their curve and lookup are untouched.
//!
//! WHICH EDITS
//!
//! An edit that put a film look on before this keeps its look. A LUT is fed
//! what its file declares when the edit carries `"agLutDeclaredInput": true`,
//! which the Looks tab writes whenever it puts a LUT on or previews one
//! (src/argentum/looks/filmLookInput.ts), and Copy settings carries with the
//! LUT. Their Looks swatches (`generate_lut_previews`) show a LUT as it goes
//! on now. Only the scene-referred path reads it; a LUT of the user's goes
//! after the rendering, as before.

use serde_json::Value;

/// The key in an edit that marks its LUT as fed what the LUT's file declares.
pub const KEY: &str = "agLutDeclaredInput";

/// The `lutPath` of their LUT swatches (`generate_lut_previews` in
/// lut_processing.rs), which are rendered from no edit but this.
const SWATCH_LUT_PATH: &str = "preview";

/// Whether the edit's LUT is fed what its file declares, as the shader reads
/// it (`ag_film_lut_input` in their `GlobalAdjustments`): 1 or 0. A swatch
/// shows the LUT as it goes on now.
pub fn declared_input(js: &Value) -> u32 {
    u32::from(js[KEY].as_bool() == Some(true) || js["lutPath"].as_str() == Some(SWATCH_LUT_PATH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::dng_standard::gpu::{centre, device, image_of, render_with_lut};
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn only_a_marked_edit_is_fed_the_declared_input() {
        assert_eq!(declared_input(&json!({ KEY: true })), 1);
        assert_eq!(declared_input(&json!({ KEY: false })), 0);
        assert_eq!(declared_input(&json!({ "lutIsSceneReferred": true })), 0);
        assert_eq!(declared_input(&Value::Null), 0);
        // Their swatch, as generate_lut_previews writes it.
        let swatch =
            json!({ "lutPath": "preview", "lutIntensity": 100, "lutIsSceneReferred": true });
        assert_eq!(declared_input(&swatch), 1);
    }

    /// A LUT whose output is its input, so the render shows what the LUT was
    /// handed.
    fn identity(size: u32) -> crate::lut_processing::Lut {
        let step = 1.0 / (size - 1) as f32;
        let mut data = Vec::with_capacity((size * size * size * 3) as usize);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.extend([r as f32 * step, g as f32 * step, b as f32 * step]);
                }
            }
        }
        crate::lut_processing::Lut { size, data }
    }

    /// Their `linear_to_vlog` (shader.wgsl), Panasonic's V-Log curve.
    fn vlog(v: f32) -> f32 {
        let v = v.max(0.0);
        if v <= 0.01 {
            5.6 * v + 0.125
        } else {
            0.241514 * (v + 0.00873).log10() + 0.598206
        }
    }

    /// Linear sRGB to linear V-Gamut, made from the two sets of primaries by
    /// their `primaries_to_xyz_matrix` (image_processing.rs), the definition
    /// the shader's constant is checked against.
    fn srgb_to_v_gamut() -> glam::Mat3 {
        use crate::image_processing::{PRIMARIES_SRGB, WP_D65, primaries_to_xyz_matrix};
        use glam::Vec2;
        // Panasonic, "V-Log/V-Gamut Reference Manual", 2014: V-Gamut's
        // primaries; its white is D65.
        let v_gamut = [
            Vec2::new(0.730, 0.280),
            Vec2::new(0.165, 0.840),
            Vec2::new(0.100, -0.030),
        ];
        primaries_to_xyz_matrix(&v_gamut, WP_D65).inverse()
            * primaries_to_xyz_matrix(&PRIMARIES_SRGB, WP_D65)
    }

    /// The shader's constant is the conversion the primaries define, to the
    /// precision it is written in.
    #[test]
    fn the_shaders_matrix_is_v_gamuts() {
        const SOURCE: &str = include_str!("../shaders/modules.wgsl");
        let start = SOURCE
            .find("const AG_SRGB_TO_V_GAMUT")
            .expect("the constant is in modules.wgsl");
        let body = &SOURCE[start..start + SOURCE[start..].find(");").expect("its end")];
        let columns: Vec<f32> = body
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter(|s| s.contains('.'))
            .map(|s| s.parse().expect("a number"))
            .collect();
        assert_eq!(columns.len(), 9, "{body}");
        let written = glam::Mat3::from_cols_slice(&columns);
        let defined = srgb_to_v_gamut();
        for (w, d) in written.to_cols_array().iter().zip(defined.to_cols_array()) {
            assert!((w - d).abs() < 1e-6, "{written} is not {defined}");
        }
        for row in 0..3 {
            let sum: f32 = (0..3).map(|col| written.col(col)[row]).sum();
            assert!((sum - 1.0).abs() < 1e-6, "row {row} sums to {sum}");
        }
    }

    /// Greys, and colours at full and partial strength, in linear sRGB.
    const PATCHES: [[f32; 3]; 10] = [
        [0.0, 0.0, 0.0],
        [0.18, 0.18, 0.18],
        [0.9, 0.9, 0.9],
        [0.6, 0.05, 0.03],
        [0.05, 0.5, 0.08],
        [0.04, 0.12, 0.7],
        [0.2, 0.35, 0.7],
        [0.7, 0.4, 0.25],
        [0.55, 0.1, 0.6],
        [1.0, 0.85, 0.1],
    ];

    /// The film look's input, through the whole render with a LUT that hands
    /// back what it is given: V-Log over V-Gamut for a marked edit, V-Log over
    /// the working RGB for one put on before.
    #[test]
    #[ignore = "needs a GPU; run with --ignored"]
    fn the_shader_feeds_a_marked_film_look_v_gamut() {
        let ctx = device().expect("no wgpu adapter");
        let image = image_of(&PATCHES);
        let lut = Arc::new(identity(65));
        let m = srgb_to_v_gamut();
        for marked in [false, true] {
            let js = json!({
                "rawToneRendering": "default",
                "agSharpen": { "capture": false, "amount": 0 },
                "lutPath": "identity",
                "lutIntensity": 100,
                "lutIsSceneReferred": true,
                KEY: marked,
            });
            let got = render_with_lut(
                &ctx,
                &image,
                js,
                crate::white_balance::WhiteBalance::reference(),
                Some(lut.clone()),
            );
            let mut worst = 0.0f32;
            for (i, c) in PATCHES.iter().enumerate() {
                let fed = if marked {
                    (m * glam::Vec3::from_array(*c)).to_array()
                } else {
                    *c
                };
                let want = fed.map(vlog);
                let g = centre(&got, i);
                let miss = (0..3).map(|k| (g[k] - want[k]).abs()).fold(0.0, f32::max) * 255.0;
                if miss > 1.0 {
                    eprintln!("marked {marked}, patch {c:?}: shader {g:?}, expected {want:?}");
                }
                worst = worst.max(miss);
            }
            eprintln!("marked {marked}: worst {worst:.2}/255");
            assert!(
                worst < 1.5,
                "marked {marked}: {worst:.2}/255 from the reference"
            );
        }
    }
}
