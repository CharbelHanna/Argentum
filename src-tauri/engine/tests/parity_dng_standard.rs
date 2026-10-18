//! `ag_dng_standard` against the app's DNG Standard.
//!
//! The app's side is its own files: `mods/dng_standard.rs` and
//! `mods/profiles.rs`, compiled unchanged by the reference crate, and
//! `ag_dng_standard` in `shaders/modules.wgsl`, run on the GPU.
//!
//! - Each body's exposure offset, by make and model: identical.
//! - The curve: the app makes 16 points of it for its shader; at each of
//!   them the engine's exact curve gives the same value, every bit.
//! - Everything else the shader does - to ProPhoto and clipped, RGBTone,
//!   back to sRGB, the sRGB curve - given a curve its spline draws exactly
//!   (a straight line through 16 points): within a hundredth of an 8-bit
//!   level, any channel. Only the GPU's `pow` differs.
//! - The picture: between its 16 points the app's shader draws a spline,
//!   which its own test holds within half an 8-bit level of the exact curve
//!   the engine reads. On screen that half level grows where a channel is
//!   near black and the sRGB curve is steepest - a saturated colour's
//!   smallest channel. Measured: 99.5% of channels within half a level, none
//!   more than 4.9 levels. Stated: 98% of channels within half a
//!   level, none more than 6.

mod common;

use argentum_engine::Source;
use argentum_engine::colour::{srgb_decode, srgb_encode};
use argentum_engine::tools::ag_dng_standard::{self, Tone};
use argentum_engine_reference::mods::dng_standard as app;
use common::app_shader::{AppShader, Job};

fn app_offset(make: &str, model: &str) -> f32 {
    app::baseline(&app::Facts {
        make: make.into(),
        model: model.into(),
        ..Default::default()
    })
    .ev
}

#[test]
fn each_bodys_offset_is_the_apps() {
    let table = include_str!("../../src/mods/dng_baseline.tsv");
    let mut asked = vec![
        ("Nobody".to_string(), "X1".to_string()),
        ("Canon".to_string(), "EOS 9999D".to_string()),
        ("NIKON CORPORATION".to_string(), "NIKON D750".to_string()),
        (String::new(), String::new()),
    ];
    for line in table
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let fields: Vec<&str> = line.split('\t').collect();
        for name in fields[1].split('|') {
            asked.push((fields[0].to_string(), name.to_string()));
            asked.push((fields[0].to_uppercase(), format!("{} {name}", fields[0])));
        }
    }
    for (make, model) in &asked {
        assert_eq!(
            ag_dng_standard::baseline(make, model).to_bits(),
            app_offset(make, model).to_bits(),
            "{make} / {model}"
        );
    }
    eprintln!("\n  {} make and model pairs, all the app's", asked.len());
}

#[test]
fn a_photos_offset_is_its_bodys() {
    eprintln!();
    for path in common::raws("a_photos_offset_is_its_bodys") {
        let bytes = std::fs::read(&path).unwrap();
        let source = Source::decode(&bytes).unwrap();
        let edit = argentum_engine::standard().defaults_for(&source);
        let ours = edit.tools["ag_dng_standard"].settings["baseline_exposure_ev"]
            .as_f64()
            .unwrap() as f32;
        let facts = app::facts(&bytes).unwrap();
        let app_full = app::baseline(&facts).ev;
        let name = path.file_name().unwrap().to_string_lossy();
        eprintln!(
            "  {name:<52} {} {}, ISO {:?}: ours {ours:+.2} EV, the app's {app_full:+.2} EV",
            facts.make, facts.model, facts.iso
        );
        // By make and model, the app's; ISO and a DNG's own offset are not
        // read by the engine yet.
        assert_eq!(ours, app_offset(&facts.make, &facts.model), "{name}");
    }
}

#[test]
fn the_curve_is_the_apps_at_every_point_it_gives_its_shader() {
    for ev in [-1.5, -0.5, -0.15, 0.0, 0.3, 0.5, 0.85, 1.5] {
        let tone = Tone::new(ev);
        for (i, point) in app::curve_points(ev).iter().enumerate() {
            let x = (i as f32 / 15.0).powf(1.3);
            assert_eq!(point.x, x * 255.0);
            let ours = srgb_encode(tone.apply(srgb_decode(x))) * 255.0;
            assert_eq!(ours.to_bits(), point.y.to_bits(), "{ev:+} EV, point {i}");
        }
    }
}

fn app_shader(gpu: std::sync::Arc<argentum_engine::Gpu>) -> AppShader {
    AppShader::new(
        gpu,
        r#"
        var points: array<Point, 16>;
        for (var k = 0u; k < 16u; k = k + 1u) {
            let q = ag_test_params[k];
            points[k] = Point(q.x, q.y, 0.0, 0.0);
        }
        return vec4<f32>(ag_dng_standard(v.xyz, points, 16u), 0.0);
        "#,
    )
}

fn as_params(points: &[(f32, f32)]) -> Vec<[f32; 4]> {
    points.iter().map(|&(x, y)| [x, y, 0.0, 0.0]).collect()
}

#[test]
fn around_the_curve_the_shader_is_the_apps() {
    let Some(gpu) = common::gpu("around_the_curve_the_shader_is_the_apps") else {
        return;
    };
    let shader = app_shader(gpu);
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    // A straight line, spaced as the app spaces its points: its spline is
    // the line itself, so the curve is the same on both sides.
    let line: Vec<(f32, f32)> = (0..16)
        .map(|i| {
            let x = (i as f32 / 15.0).powf(1.3) * 255.0;
            (x, x)
        })
        .collect();
    let theirs = shader.run(&Job {
        inputs: &inputs,
        params: &as_params(&line),
        ..Default::default()
    });
    let mut worst = 0.0f32;
    for (p, theirs) in rgb.pixels.iter().zip(&theirs) {
        let ours =
            ag_dng_standard::dng_standard(*p, |v| srgb_decode(srgb_encode(v))).map(srgb_encode);
        for c in 0..3 {
            let levels = (ours[c] - theirs[c]).abs() * 255.0;
            worst = worst.max(levels);
            assert!(levels <= 0.01, "{p:?}: ours {ours:?}, the app's {theirs:?}");
        }
    }
    eprintln!(
        "
  worst {worst:.4} of an 8-bit level"
    );
}

#[test]
fn the_picture_is_the_apps_within_its_spline() {
    let Some(gpu) = common::gpu("the_picture_is_the_apps_within_its_spline") else {
        return;
    };
    let shader = app_shader(gpu);
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    eprintln!();
    for ev in [-0.5, 0.0, 0.3, 0.85] {
        let points: Vec<(f32, f32)> = app::curve_points(ev).iter().map(|p| (p.x, p.y)).collect();
        let theirs = shader.run(&Job {
            inputs: &inputs,
            params: &as_params(&points),
            ..Default::default()
        });
        let tone = Tone::new(ev);
        let mut worst = (0.0f32, [0.0; 3]);
        let mut over_half = 0;
        for (p, theirs) in rgb.pixels.iter().zip(&theirs) {
            let ours = ag_dng_standard::dng_standard(*p, |v| tone.apply(v)).map(srgb_encode);
            for c in 0..3 {
                let levels = (ours[c] - theirs[c]).abs() * 255.0;
                if levels > worst.0 {
                    worst = (levels, *p);
                }
                over_half += usize::from(levels > 0.5);
            }
        }
        let channels = rgb.pixels.len() * 3;
        eprintln!(
            "  {ev:+.2} EV: worst {:.2} of an 8-bit level at {:?}; {over_half} of {channels} channels over half a level",
            worst.0, worst.1
        );
        assert!(worst.0 <= 6.0, "{ev:+} EV: {worst:?}");
        assert!(
            over_half * 50 <= channels,
            "{ev:+} EV: {over_half} channels over half a level"
        );
    }
}
