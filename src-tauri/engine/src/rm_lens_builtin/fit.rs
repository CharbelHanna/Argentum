//! The maths more than one make's reader shares: the radii a maker's model is
//! sampled at, and the sampling.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase (MIT), `crates/rawmakase-model/src/lens/embedded.rs`:
//! - `knots` and `maker_model` @ af35e25, as written.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - `maker_model` takes the developed image's width and height in pixels
//!   where RAWmakase takes its `Metadata`'s decoded size: radius 1 is the
//!   developed image's half diagonal, as in every reader here.
//! - `knots` also serves the DNG reader, whose own copy (`dng_knots`, from
//!   `crates/rawmakase-model/src/dng.rs`) was the same 33 radii.

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
