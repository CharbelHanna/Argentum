//! What the integration tests share: local RAWs from the environment, a GPU
//! if there is one, and synthetic sensor data for everything else.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use argentum_engine::rawler::cfa::{CFA, PlaneColor};
use argentum_engine::rawler::decoders::{Camera, Orientation};
use argentum_engine::rawler::rawimage::{
    BlackLevel, CFAConfig, RawImage, RawImageData, RawPhotometricInterpretation, WhiteLevel,
};
use argentum_engine::{Gpu, Source};

/// The RAWs named by `AG_RAWS` (paths separated by `;`) and `AG_RAW`, as the
/// app's own tests name theirs. Empty, with a line saying how to run it, when
/// neither is set.
pub fn raws(test: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::env::var("AG_RAWS")
        .unwrap_or_default()
        .split(';')
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .collect();
    if let Ok(one) = std::env::var("AG_RAW")
        && !one.trim().is_empty()
        && !out.contains(&PathBuf::from(&one))
    {
        out.push(PathBuf::from(one));
    }
    if out.is_empty() {
        eprintln!(
            "{test}: SKIPPED on real photos - set AG_RAW to a RAW file, or AG_RAWS to several separated by ';'"
        );
    }
    out
}

/// The first of `raws`.
pub fn raw(test: &str) -> Option<PathBuf> {
    raws(test).into_iter().next()
}

/// A GPU of the engine's own, or `None` with a line saying the test skipped.
pub fn gpu(test: &str) -> Option<Arc<Gpu>> {
    match Gpu::new() {
        Ok(gpu) => {
            eprintln!(
                "{test}: GPU {} ({:?})",
                gpu.adapter.name, gpu.adapter.backend
            );
            Some(Arc::new(gpu))
        }
        Err(e) => {
            eprintln!("{test}: SKIPPED - no GPU adapter ({e})");
            None
        }
    }
}

pub fn open(path: &PathBuf) -> Source {
    Source::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A small RGGB mosaic with sensor levels: black 512, white 15000, and
/// `value(row, col)` at each photosite. No colour matrix, so the develop
/// leaves the colours as the sensor's.
pub fn bayer(width: usize, height: usize, value: impl Fn(usize, usize) -> u16) -> RawImage {
    let cfa = CFA::new("RGGB");
    let camera = Camera {
        cfa: cfa.clone(),
        ..Default::default()
    };
    let data = (0..height)
        .flat_map(|r| (0..width).map(move |c| (r, c)))
        .map(|(r, c)| value(r, c))
        .collect();
    RawImage {
        camera,
        make: "Synthetic".into(),
        model: "Bayer".into(),
        clean_make: "Synthetic".into(),
        clean_model: "Bayer".into(),
        width,
        height,
        cpp: 1,
        bps: 14,
        wb_coeffs: [2.0, 1.0, 1.5, f32::NAN],
        whitelevel: WhiteLevel::new(vec![15000]),
        blacklevel: BlackLevel::new(&[512u32], 1, 1, 1),
        xyz_to_cam: [[0.0; 3]; 4],
        photometric: RawPhotometricInterpretation::Cfa(CFAConfig::new(
            &cfa,
            &PlaneColor::new("RGB"),
        )),
        active_area: None,
        crop_area: None,
        blackareas: Vec::new(),
        orientation: Orientation::Normal,
        data: RawImageData::Integer(data),
        color_matrix: HashMap::new(),
        dng_tags: HashMap::new(),
    }
}

/// A test scene on a mosaic: a smooth gradient, a few sharp edges, some
/// pseudo-random texture, and a sun-like disc that clips - green first, as a
/// daylight frame does, with a core where everything clips.
pub fn scene(width: usize, height: usize) -> RawImage {
    let (cx, cy) = (width as f32 * 0.7, height as f32 * 0.35);
    let r = width.min(height) as f32 * 0.18;
    bayer(width, height, move |row, col| {
        let colour = match (row % 2, col % 2) {
            (0, 0) => 0,
            (1, 1) => 2,
            _ => 1,
        };
        let x = col as f32 / width as f32;
        let y = row as f32 / height as f32;
        let mut light = 0.08 + 0.25 * x + 0.1 * y;
        if (col / 24 + row / 24) % 2 == 0 {
            light += 0.12;
        }
        // Texture: a cheap hash, a few percent.
        let n = ((row * 7919 + col * 104_729) % 1013) as f32 / 1013.0;
        light += 0.03 * n;
        let d = ((col as f32 - cx).powi(2) + (row as f32 - cy).powi(2)).sqrt();
        if d < r {
            light += 2.0 * (1.0 - d / r) + 0.5;
        }
        // Per-colour sensitivity, green the strongest: green clips first.
        let gain = [0.55, 1.0, 0.7][colour];
        let v = 512.0 + light * gain * 14488.0;
        v.min(15000.0) as u16
    })
}

/// A synthetic photo, keyed by its size.
pub fn source(width: usize, height: usize) -> Source {
    Source::new((width * 31 + height) as u64, scene(width, height))
}
