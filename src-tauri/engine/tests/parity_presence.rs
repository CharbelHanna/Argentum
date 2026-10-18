//! `rr_presence` against the app's `apply_creative_color`
//! (`shaders/shader.wgsl`), run on the GPU.
//!
//! Stated tolerance: 2 parts in 10^5 of the value, or 10^-6 absolute near
//! zero. The function goes through the sRGB curve and HSV to weigh
//! vibrance; the GPU's `pow` differs from Rust's in the last bits.

mod common;

use argentum_engine::tools::rr_presence;
use common::app_shader::{AppShader, Job};

#[test]
fn vibrance_and_saturation_are_the_apps_shader() {
    let Some(gpu) = common::gpu("vibrance_and_saturation_are_the_apps_shader") else {
        return;
    };
    let shader = AppShader::new(
        gpu,
        "return vec4<f32>(apply_creative_color(v.xyz, ag_test_params[0].x, ag_test_params[0].y), 0.0);",
    );
    let rgb = common::colours();
    let inputs: Vec<[f32; 4]> = rgb.pixels.iter().map(|p| [p[0], p[1], p[2], 0.0]).collect();
    let mut worst = 0.0f32;
    // As the app's sliders, then as the shader takes them.
    for (vibrance, saturation) in [
        (60.0, 0.0),
        (-70.0, 0.0),
        (0.0, 45.0),
        (0.0, -100.0),
        (35.0, -20.0),
        (100.0, 100.0),
    ] {
        let (sat, vib) = (saturation / 100.0, vibrance / 100.0);
        let theirs = shader.run(&Job {
            inputs: &inputs,
            params: &[[sat, vib, 0.0, 0.0]],
            ..Default::default()
        });
        for (p, b) in rgb.pixels.iter().zip(&theirs) {
            let ours = rr_presence::creative_color(*p, sat, vib);
            for c in 0..3 {
                let miss = (ours[c] - b[c]).abs();
                let allowed = b[c].abs() * 2e-5 + 1e-6;
                worst = worst.max(miss / allowed);
                assert!(
                    miss <= allowed,
                    "vibrance {vibrance}, saturation {saturation}, {p:?}: ours {ours:?}, the app's {b:?}"
                );
            }
        }
    }
    eprintln!("\n  worst miss: {worst:.2} of the tolerance");
}
