//! Pass test 3c: `ag_exposure_wb` is exact against a reference computation.
//!
//! The reference is the tool's stated formula, written out here on its own:
//! each channel times (2 to the exposure, times that channel's multiplier),
//! the factor in f32. Exact means every bit.

mod common;

use argentum_engine::tools::ag_exposure_wb::{self, AgExposureWb};
use argentum_engine::tools::rt_capture_sharpen;
use argentum_engine::{Context, GpuSlot, Image, Pipeline, Rgb, Size, Source, Tool};

fn reference(rgb: &Rgb, exposure: f32, multipliers: [f32; 3]) -> Vec<[f32; 3]> {
    let gain = 2.0f32.powf(exposure);
    let factor = [
        gain * multipliers[0],
        gain * multipliers[1],
        gain * multipliers[2],
    ];
    rgb.pixels
        .iter()
        .map(|p| [p[0] * factor[0], p[1] * factor[1], p[2] * factor[2]])
        .collect()
}

/// Values from black to far above white, including the awkward ones.
fn picture() -> Rgb {
    let mut seed = 0x2545_f491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let mut pixels: Vec<[f32; 3]> = (0..4096)
        .map(|_| [next() * 2.0, next() * 0.5, next().powi(4) * 900.0])
        .collect();
    pixels.extend([
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [f32::MIN_POSITIVE, 1e-30, 1e-7],
        [1000.0, 999.9, 0.18],
    ]);
    Rgb::new(4100, 1, pixels)
}

fn run(rgb: &Rgb, settings: &ag_exposure_wb::Settings) -> Option<Vec<[f32; 3]>> {
    let source = Source::new(0, common::bayer(16, 16, |_, _| 0));
    let slot = GpuSlot::new();
    let cx = Context::new::<AgExposureWb>(&source, 0, &slot);
    AgExposureWb
        .process(settings, &Image::Rgb(rgb.clone()), &cx)
        .unwrap()
        .map(|image| image.as_rgb().unwrap().pixels.clone())
}

#[test]
fn exact_against_the_reference_computation() {
    let rgb = picture();
    let cases = [
        (1.0, [1.0, 1.0, 1.0]),
        (-2.5, [1.9, 1.0, 1.4]),
        (0.0, [0.5, 1.0, 2.0]),
        (3.3, [0.73, 1.02, 1.61]),
        (-10.0, [20.0, 0.05, 1.0]),
        (1.0 / 3.0, [1.0 / 3.0, 1.0, 3.0]),
    ];
    for (exposure, multipliers) in cases {
        let [red, green, blue] = multipliers;
        let settings = ag_exposure_wb::Settings {
            exposure,
            red,
            green,
            blue,
        };
        let ours = run(&rgb, &settings).expect("something to do");
        let theirs = reference(&rgb, exposure, multipliers);
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            for c in 0..3 {
                assert_eq!(
                    a[c].to_bits(),
                    b[c].to_bits(),
                    "exposure {exposure} x {multipliers:?}, pixel {i} channel {c}: {} vs {}",
                    a[c],
                    b[c]
                );
            }
        }
        // And the formula is the right one: within float rounding of the
        // same thing worked out in f64.
        for (a, p) in ours.iter().zip(&rgb.pixels) {
            for c in 0..3 {
                let exact = p[c] as f64 * 2f64.powf(exposure as f64) * multipliers[c] as f64;
                let tolerance = exact.abs() * 4.0 * f32::EPSILON as f64 + 1e-37;
                assert!(
                    (a[c] as f64 - exact).abs() <= tolerance,
                    "{} vs {exact}",
                    a[c]
                );
            }
        }
    }
}

#[test]
fn the_defaults_do_nothing_and_say_so() {
    assert!(run(&picture(), &ag_exposure_wb::Settings::default()).is_none());
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
    let before = p.render(&source, Size::Full).unwrap();
    let settings = ag_exposure_wb::Settings {
        exposure: 0.7,
        red: 1.3,
        green: 1.0,
        blue: 0.8,
    };
    p.set::<AgExposureWb>(settings.clone()).unwrap();
    let after = p.render(&source, Size::Full).unwrap();
    let theirs = reference(before.as_rgb().unwrap(), 0.7, [1.3, 1.0, 0.8]);
    assert_eq!(after.as_rgb().unwrap().pixels, theirs);
}
