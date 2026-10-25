//! The maths and the small helpers more than one make's reader shares: the
//! radii a maker's model is sampled at, the sampling, the least-squares solve
//! that fits a polynomial to a camera's table, and IFD0's Make.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase (MIT), `crates/rawmakase-model/src/lens/embedded.rs`:
//! - `knots` and `maker_model` @ af35e25, as written.
//! - `solve4` @ d04ef4f, as written.
//! - `make` @ 0b7d217 (in d04ef4f's Panasonic reader, the same lines inline).
//!
//! WHAT CHANGED IN THE COPY
//!
//! - `maker_model` takes the developed image's width and height in pixels
//!   where RAWmakase takes its `Metadata`'s decoded size: radius 1 is the
//!   developed image's half diagonal, as in every reader here.
//! - `knots` also serves the DNG reader, whose own copy (`dng_knots`, from
//!   `crates/rawmakase-model/src/dng.rs`) was the same 33 radii.
//! - `make` reads rawler's IFD in place of `rawmakase-tiff`'s.

use rawler::formats::tiff::IFD;
use rawler::tags::TiffCommonTag;

use super::Radial;

// RAWmakase af35e25: crates/rawmakase-model/src/lens/embedded.rs
/// Radius samples from the centre to the developed image's corner.
pub(super) fn knots() -> Vec<f32> {
    (0..=32).map(|i| i as f32 / 32.).collect()
}

// RAWmakase af35e25: crates/rawmakase-model/src/lens/embedded.rs
/// A maker's radial model in its own radius unit, `unit` pixels, sampled at the
/// radii of a developed image of `size_px`: `green` maps an output radius to
/// green's source radius, `chromatic` red's and blue's scale relative to green at
/// green's source radius.
pub(super) fn maker_model(
    size_px: [f64; 2],
    unit: f64,
    green: &dyn Fn(f64) -> Option<f64>,
    chromatic: Option<[&dyn Fn(f64) -> f64; 2]>,
) -> (Option<Radial>, Option<[Radial; 2]>) {
    let half = size_px[0].hypot(size_px[1]) * 0.5;
    if !(half > 0. && unit > 0.) {
        return (None, None);
    }
    // The ratio's limit at the centre is its value just beside it.
    let x = |r: f32| (f64::from(r) * half / unit).max(1e-4);
    let sources: Option<Vec<f64>> = knots().into_iter().map(|r| green(x(r))).collect();
    let Some(sources) = sources.filter(|s| s.windows(2).all(|p| p[0] < p[1])) else {
        return (None, None);
    };
    let curve = |f: &dyn Fn(usize, f64) -> f64| {
        let values = (knots().into_iter().enumerate())
            .map(|(i, r)| f(i, x(r)) as f32)
            .collect();
        Radial::new(knots(), values)
    };
    let changes = |r: &Radial, by: f32| r.values.iter().any(|v| (v - 1.).abs() > by);
    let distortion = curve(&|i, x| sources[i] / x).filter(|r| changes(r, 1e-6));
    let chromatic = chromatic
        .and_then(|[red, blue]| {
            Some([
                curve(&|i, _| red(sources[i]))?,
                curve(&|i, _| blue(sources[i]))?,
            ])
        })
        .filter(|[a, b]| changes(a, 1e-7) || changes(b, 1e-7));
    (distortion, chromatic)
}

// RAWmakase d04ef4f: crates/rawmakase-model/src/lens/embedded.rs
/// Solves the 4×4 system in the augmented matrix `m` by Gaussian elimination with
/// partial pivoting, or `None` when it is singular.
pub(super) fn solve4(mut m: [[f64; 5]; 4]) -> Option<[f64; 4]> {
    for col in 0..4 {
        let pivot = (col..4).max_by(|a, b| m[*a][col].abs().total_cmp(&m[*b][col].abs()))?;
        m.swap(col, pivot);
        if m[col][col].abs() < 1e-300 {
            return None;
        }
        let top = m[col];
        for row in &mut m[col + 1..] {
            let f = row[col] / top[col];
            for (x, t) in row.iter_mut().zip(top).skip(col) {
                *x -= f * t;
            }
        }
    }
    let mut x = [0.; 4];
    for row in (0..4).rev() {
        let rest: f64 = (row + 1..4).map(|k| m[row][k] * x[k]).sum();
        x[row] = (m[row][4] - rest) / m[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

// RAWmakase 0b7d217: crates/rawmakase-model/src/lens/embedded.rs
/// IFD0's Make, in lower case.
pub(super) fn make(ifd0: &IFD) -> String {
    ifd0.get_entry(TiffCommonTag::Make)
        .and_then(|e| e.value.as_string())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}
