//! `ag_exposure` is exact against its formula, and agrees with the app's
//! `apply_linear_exposure` (shader.wgsl) run on the GPU.
//!
//! The formula: each channel times 2 to the exposure, the factor worked out
//! in f32. Exact means every bit. The app's shader works the factor out per
//! pixel on the GPU, whose `pow` may differ from Rust's in the last bits:
//! the stated tolerance is 4 f32 steps of the value (4 x 2^-23, relative).

mod common;

use argentum_engine::tools::ag_dng_standard::{self, AgDngStandard};
use argentum_engine::tools::ag_exposure::{self, AgExposure};
use argentum_engine::tools::rt_capture_sharpen;
use argentum_engine::{Context, GpuSlot, Image, Pipeline, Rgb, Size, Source, Tool};
use common::app_shader::{AppShader, Job};

const CASES_EV: [f32; 6] = [1.0, -2.5, 0.0, 3.3, -5.0, 1.0 / 3.0];

fn reference(rgb: &Rgb, exposure_ev: f32) -> Vec<[f32; 3]> {
    let gain = 2.0f32.powf(exposure_ev);
    rgb.pixels
        .iter()
        .map(|p| [p[0] * gain, p[1] * gain, p[2] * gain])
        .collect()
}

fn run(rgb: &Rgb, settings: &ag_exposure::Settings) -> Option<Vec<[f32; 3]>> {
    let source = Source::new(0, common::bayer(16, 16, |_, _| 0));
    let slot = GpuSlot::new();
    let cx = Context::new::<AgExposure>(&source, 0, &slot);
    AgExposure
        .process(settings, &Image::Rgb(rgb.clone()), &cx)
        .unwrap()
        .map(|image| image.as_rgb().unwrap().pixels.clone())
}

#[test]
fn exact_against_its_formula() {
    let rgb = common::colours();
    for exposure_ev in CASES_EV {
        let settings = ag_exposure::Settings { exposure_ev };
        let theirs = reference(&rgb, exposure_ev);
        let Some(ours) = run(&rgb, &settings) else {
            assert_eq!(exposure_ev, 0.0, "only no exposure does nothing");
            continue;
        };
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            for c in 0..3 {
                assert_eq!(
                    a[c].to_bits(),
                    b[c].to_bits(),
                    "{exposure_ev} EV, pixel {i} channel {c}: {} vs {}",
                    a[c],
                    b[c]
                );
            }
        }
    }
}

#[test]
fn the_apps_shader_agrees() {
    let Some(gpu) = common::gpu("the_apps_shader_agrees") else {
        return;
    };
    let shader = AppShader::new(
        gpu,
        "return vec4<f32>(apply_linear_exposure(v.xyz, ag_test_params[0].x), 0.0);",
    );
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    for exposure_ev in CASES_EV {
        let theirs = shader.run(&Job {
            inputs: &inputs,
            params: &[[exposure_ev, 0.0, 0.0, 0.0]],
            ..Default::default()
        });
        let ours = run(&rgb, &ag_exposure::Settings { exposure_ev }).unwrap_or(rgb.pixels.clone());
        for (a, b) in ours.iter().zip(&theirs) {
            for c in 0..3 {
                let tolerance = b[c].abs() * 4.0 * f32::EPSILON + f32::MIN_POSITIVE;
                assert!(
                    (a[c] - b[c]).abs() <= tolerance,
                    "{exposure_ev} EV: ours {}, the app's {}",
                    a[c],
                    b[c]
                );
            }
        }
    }
}

#[test]
fn exact_through_the_pipeline() {
    let source = common::source(320, 240);
    let mut p = Pipeline::standard().unwrap();
    p.set::<rt_capture_sharpen::RtCaptureSharpen>(rt_capture_sharpen::Settings {
        capture: false,
        ..Default::default()
    })
    .unwrap();
    // Everything after exposure off or at the photo's own defaults, so the
    // render is exposure's own result.
    p.set::<AgDngStandard>(ag_dng_standard::Settings {
        enabled: false,
        ..Default::default()
    })
    .unwrap();
    let before = p.render(&source, Size::Full).unwrap();
    p.set::<AgExposure>(ag_exposure::Settings { exposure_ev: 0.7 })
        .unwrap();
    let after = p.render(&source, Size::Full).unwrap();
    let theirs = reference(before.as_rgb().unwrap(), 0.7);
    assert_eq!(after.as_rgb().unwrap().pixels, theirs);
}
