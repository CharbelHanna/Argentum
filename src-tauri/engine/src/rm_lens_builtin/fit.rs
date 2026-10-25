//! The maths the readers sample a camera's tables with, defined once for
//! every make that needs it.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `44b21d5` (MIT), `crates/rawmakase-model/src/lens/embedded.rs`:
//! `knots` and `spline`, as written.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - Nothing in the maths. `knots` is also the 33 radii a DNG's opcodes are
//!   sampled at (`dng.rs`), where RAWmakase's `dng.rs` has its own.

/// Radius samples from the centre to the developed image's corner.
// RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
pub(super) fn knots() -> Vec<f32> {
    (0..=32).map(|i| i as f32 / 32.).collect()
}

/// A natural cubic spline through `v`, at knots spread evenly over [0, 1].
/// `v` holds at least two values.
// RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
pub(super) fn spline(v: &[f64]) -> impl Fn(f64) -> f64 + '_ {
    let n = v.len();
    let h = 1. / (n - 1) as f64;
    // Second derivatives, zero at both ends (Thomas algorithm).
    let mut m = vec![0.; n];
    if n > 2 {
        let (mut c, mut d) = (vec![0.; n], vec![0.; n]);
        for i in 1..n - 1 {
            let rhs = 6. * (v[i + 1] - 2. * v[i] + v[i - 1]) / (h * h);
            let w = 4. - c[i - 1];
            c[i] = 1. / w;
            d[i] = (rhs - d[i - 1]) / w;
        }
        for i in (1..n - 1).rev() {
            m[i] = d[i] - c[i] * m[i + 1];
        }
    }
    move |r: f64| {
        let i = ((r / h) as usize).min(n - 2);
        let t = r - i as f64 * h;
        let u = h - t;
        (m[i] * u * u * u + m[i + 1] * t * t * t) / (6. * h)
            + (v[i] / h - m[i] * h / 6.) * u
            + (v[i + 1] / h - m[i + 1] * h / 6.) * t
    }
}
