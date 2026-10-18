//! `rr_white_balance` against the app's white balance.
//!
//! The app's side is its own files: `white_balance.rs` and
//! `raw_processing.rs`, compiled unchanged by the reference crate, and
//! `apply_white_balance` in `shaders/shader.wgsl`, run on the GPU.
//!
//! - The as-shot light, read from the same RAW: identical, every bit.
//! - Kelvin and tint to cone gains, and the RGB-to-LMS matrix: identical.
//! - The pixels: the engine folds the shader's three steps into one matrix,
//!   built in f64. Stated tolerance: 2 parts in 10^6 of the pixel's largest
//!   channel, in or out, or 10^-7 absolute - a few f32 steps of what the
//!   matrix sums, nothing that reaches 8 bits. Relative to the channel itself
//!   would be wrong where a matrix row nearly cancels out.

mod common;

use argentum_engine::Source;
use argentum_engine::colour;
use argentum_engine::tools::rr_white_balance::{self, RrWhiteBalance, WhiteBalance};
use argentum_engine_reference::raw_processing as app_raw;
use argentum_engine_reference::white_balance as app;
use common::app_shader::{AppShader, Job, mat3_bytes};

fn theirs(wb: WhiteBalance) -> app::WhiteBalance {
    app::WhiteBalance {
        temperature: wb.temperature,
        tint: wb.tint,
    }
}

#[test]
fn the_as_shot_light_is_the_apps() {
    eprintln!();
    for path in common::raws("the_as_shot_light_is_the_apps") {
        let bytes = std::fs::read(&path).unwrap();
        let source = Source::decode(&bytes).unwrap();
        let ours = rr_white_balance::as_shot(&source.raw);
        let app = app_raw::read_as_shot_white_balance(&bytes);
        let name = path.file_name().unwrap().to_string_lossy();
        eprintln!("  {name:<52} ours {ours:?}, the app's {app:?}");
        assert_eq!(
            ours.map(|w| (w.temperature.to_bits(), w.tint.to_bits())),
            app.map(|w| (w.temperature.to_bits(), w.tint.to_bits())),
            "{name}"
        );
        // And it is where a photo starts.
        let edit = argentum_engine::standard().defaults_for(&source);
        let start = &edit.tools["rr_white_balance"].settings;
        let shot = ours.unwrap_or_else(WhiteBalance::reference);
        assert_eq!(
            start["temperature_k"].as_f64(),
            Some(shot.temperature.clamp(2000.0, 50000.0))
        );
    }
    // A sensor with no colour matrix has no reading: D65, as in the app.
    assert_eq!(rr_white_balance::as_shot(&common::scene(64, 48)), None);
}

#[test]
fn kelvin_and_tint_to_cone_gains_are_the_apps() {
    let lights = [
        (2000.0, -150.0),
        (2856.0, 0.0),
        (3200.0, 12.5),
        (4150.0, -30.0),
        (5003.0, 10.0),
        (6504.0, 0.0),
        (7500.0, 40.0),
        (12000.0, -80.0),
        (50000.0, 150.0),
    ];
    for &(t1, n1) in &lights {
        for &(t2, n2) in &lights {
            let (a, b) = (
                WhiteBalance {
                    temperature: t1,
                    tint: n1,
                },
                WhiteBalance {
                    temperature: t2,
                    tint: n2,
                },
            );
            let ours = rr_white_balance::adaptation_log_gains(a, b);
            let app = app::adaptation_log_gains(theirs(a), theirs(b));
            assert_eq!(
                ours.map(f32::to_bits),
                app.map(f32::to_bits),
                "{a:?} -> {b:?}"
            );
        }
        let (x, y) = WhiteBalance {
            temperature: t1,
            tint: n1,
        }
        .to_xy();
        let app_back = app::WhiteBalance::from_xy(x, y);
        let ours_back = WhiteBalance::from_xy(x, y);
        assert_eq!(
            (ours_back.temperature.to_bits(), ours_back.tint.to_bits()),
            (app_back.temperature.to_bits(), app_back.tint.to_bits())
        );
    }
    assert_eq!(
        rr_white_balance::rgb_to_lms()
            .to_cols_array()
            .map(f32::to_bits),
        app::rgb_to_lms().to_cols_array().map(f32::to_bits)
    );
}

#[test]
fn the_pixels_are_the_apps_shader() {
    let Some(gpu) = common::gpu("the_pixels_are_the_apps_shader") else {
        return;
    };
    let shader = AppShader::new(
        gpu,
        "return vec4<f32>(apply_white_balance(v.xyz, ag_test_params[0].xyz), 0.0);",
    );
    let to_lms = app::rgb_to_lms();
    let rows = |m: glam::Mat3| [0, 1, 2].map(|r| [0, 1, 2].map(|c| m.col(c)[r]));
    let matrices = [
        (
            &["global", "wb_rgb_to_lms_matrix"][..],
            mat3_bytes(rows(to_lms)),
        ),
        (
            &["global", "wb_lms_to_rgb_matrix"][..],
            mat3_bytes(rows(to_lms.inverse())),
        ),
    ];
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    let shot = WhiteBalance {
        temperature: 5200.0,
        tint: 6.0,
    };
    let mut worst = 0.0f32;
    for (t, tint) in [
        (2400.0, 0.0),
        (3800.0, 21.0),
        (5200.0, -40.0),
        (9000.0, 10.0),
        (30000.0, -120.0),
    ] {
        let target = WhiteBalance {
            temperature: t,
            tint,
        };
        let gains = rr_white_balance::adaptation_log_gains(shot, target);
        let app_out = shader.run(&Job {
            inputs: &inputs,
            params: &[[gains[0], gains[1], gains[2], 0.0]],
            adjustments: &matrices,
            ..Default::default()
        });
        let m = rr_white_balance::adaptation_matrix(gains);
        for (p, b) in rgb.pixels.iter().zip(&app_out) {
            let a = colour::mul(&m, *p);
            let largest = p.iter().chain(&a).fold(0.0f32, |m, v| m.max(v.abs()));
            for c in 0..3 {
                let miss = (a[c] - b[c]).abs();
                let allowed = largest * 2e-6 + 1e-7;
                worst = worst.max(miss / allowed);
                assert!(
                    miss <= allowed,
                    "{target:?}, {p:?}: ours {a:?}, the app's {b:?}"
                );
            }
        }
    }
    eprintln!("\n  worst miss: {worst:.2} of the tolerance");
}

#[test]
fn the_cameras_own_light_changes_nothing() {
    let Some(path) = common::raw("the_cameras_own_light_changes_nothing") else {
        return;
    };
    let source = common::open(&path);
    let wb = RrWhiteBalance::default();
    use argentum_engine::Tool;
    let settings = wb.defaults_for(&source);
    let slot = argentum_engine::GpuSlot::new();
    let cx = argentum_engine::Context::new::<RrWhiteBalance>(&source, 0, &slot);
    let picture = argentum_engine::Image::Rgb(common::colours());
    assert!(wb.process(&settings, &picture, &cx).unwrap().is_none());
}
