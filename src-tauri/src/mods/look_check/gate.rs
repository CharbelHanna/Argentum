//! How far a render moved from its reference, and whether that is too far.
//!
//! Robust statistics only: never a single pixel. The reference keeps a
//! thumbnail of 32 cells on the long side (about 50 pixels a cell at the
//! check's 1600) rather than the pixels, so the colour differences are
//! between cells' mean colours: "a tile" is a cell, and the frame's colour
//! statistics are over its cells. The rest - clipping, the top stops, the
//! 99.9th percentile, the blown area's colour - are kept for the whole frame
//! and compared as they are.

use argentum_engine::colour::{delta_e2000, lab, luma, srgb_decode};

use super::stats::{Look, P50, P999, Stats, Thumb};

/// The most each measure may move. A drift beyond any one of them fails.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Any cell's CIEDE2000.
    pub cell_de: f64,
    /// The cells' mean CIEDE2000.
    pub frame_de_mean: f64,
    /// Their 99th percentile.
    pub frame_de_p99: f64,
    /// The share of cells above CIEDE2000 2.
    pub frame_de_over_2: f64,
    /// The median log2 ratio of the mid-tone cells' luminance, in EV.
    pub midtone_ev: f64,
    /// Any channel's clipped share of the frame.
    pub clipped: f64,
    /// Any quarter-stop bin's share of the frame, over the top three stops.
    pub top_bin: f64,
    /// The 99.9th percentile of luminance, in EV.
    pub p999_ev: f64,
    /// How much more colourful the blown area may get, in C*.
    pub blown_chroma_rise: f64,
}

/// The expert board's numbers (2026-10-25), before the adapters' spread.
pub const BOARD: Limits = Limits {
    cell_de: 1.0,
    frame_de_mean: 0.15,
    frame_de_p99: 0.5,
    frame_de_over_2: 0.0005,
    midtone_ev: 0.03,
    clipped: 0.0005,
    top_bin: 0.001,
    p999_ev: 0.02,
    blown_chroma_rise: 2.0,
};

/// The most the same commit moved between adapters, measured 2026-10-25 on
/// the ten samples' 41 renders against references blessed on an integrated
/// GPU (Vulkan): on lavapipe (CI), WARP (DirectX 12) and the same GPU through
/// DirectX 12. A few cells moved by one stored step (cell dE at most 0.093);
/// everything else stayed below what the summary prints, given here as that
/// resolution. See `limits`.
pub const SPREAD: Limits = Limits {
    cell_de: 0.093,
    frame_de_mean: 0.0005,
    frame_de_p99: 0.0005,
    frame_de_over_2: 0.000005,
    midtone_ev: 0.00005,
    clipped: 0.000005,
    top_bin: 0.000005,
    p999_ev: 0.00005,
    blown_chroma_rise: 0.005,
};

/// What the check holds a render to: each of the board's numbers, or twice
/// what the adapters alone move it by, whichever is larger. A limit tighter
/// than the adapters' own spread would fail a render for the machine it ran
/// on.
pub fn limits() -> Limits {
    let pick = |board: f64, spread: f64| board.max(2.0 * spread);
    Limits {
        cell_de: pick(BOARD.cell_de, SPREAD.cell_de),
        frame_de_mean: pick(BOARD.frame_de_mean, SPREAD.frame_de_mean),
        frame_de_p99: pick(BOARD.frame_de_p99, SPREAD.frame_de_p99),
        frame_de_over_2: pick(BOARD.frame_de_over_2, SPREAD.frame_de_over_2),
        midtone_ev: pick(BOARD.midtone_ev, SPREAD.midtone_ev),
        clipped: pick(BOARD.clipped, SPREAD.clipped),
        top_bin: pick(BOARD.top_bin, SPREAD.top_bin),
        p999_ev: pick(BOARD.p999_ev, SPREAD.p999_ev),
        blown_chroma_rise: pick(BOARD.blown_chroma_rise, SPREAD.blown_chroma_rise),
    }
}

/// How far a render moved, in each of the measures `Limits` holds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Drift {
    pub cell_de: f64,
    pub frame_de_mean: f64,
    pub frame_de_p99: f64,
    pub frame_de_over_2: f64,
    pub midtone_ev: f64,
    pub clipped: f64,
    pub top_bin: f64,
    pub p999_ev: f64,
    pub blown_chroma_rise: f64,
}

impl Drift {
    /// The measures beyond `limits`, one line each; empty when none is.
    pub fn breaches(&self, limits: &Limits) -> Vec<String> {
        let mut out = Vec::new();
        let mut check = |name: &str, got: f64, limit: f64| {
            if got > limit {
                out.push(format!("{name} {got:.4} > {limit:.4}"));
            }
        };
        check("cell dE", self.cell_de, limits.cell_de);
        check("frame dE mean", self.frame_de_mean, limits.frame_de_mean);
        check("frame dE p99", self.frame_de_p99, limits.frame_de_p99);
        check(
            "share of cells over dE 2",
            self.frame_de_over_2,
            limits.frame_de_over_2,
        );
        check("mid-tone EV", self.midtone_ev, limits.midtone_ev);
        check("clipped share", self.clipped, limits.clipped);
        check("top-stop bin share", self.top_bin, limits.top_bin);
        check("p99.9 EV", self.p999_ev, limits.p999_ev);
        check(
            "blown-area chroma rise",
            self.blown_chroma_rise,
            limits.blown_chroma_rise,
        );
        out
    }

    /// The larger of the two in every measure.
    pub fn max(self, other: Drift) -> Drift {
        Drift {
            cell_de: self.cell_de.max(other.cell_de),
            frame_de_mean: self.frame_de_mean.max(other.frame_de_mean),
            frame_de_p99: self.frame_de_p99.max(other.frame_de_p99),
            frame_de_over_2: self.frame_de_over_2.max(other.frame_de_over_2),
            midtone_ev: self.midtone_ev.max(other.midtone_ev),
            clipped: self.clipped.max(other.clipped),
            top_bin: self.top_bin.max(other.top_bin),
            p999_ev: self.p999_ev.max(other.p999_ev),
            blown_chroma_rise: self.blown_chroma_rise.max(other.blown_chroma_rise),
        }
    }

    /// One line for a summary.
    pub fn line(&self) -> String {
        format!(
            "cell dE {:.3}, mean {:.3}, p99 {:.3}, >2 {:.3}%, mid-tones {:+.4} EV, clipped {:.3}%, top bin {:.3}%, p99.9 {:+.4} EV, blown C* {:+.2}",
            self.cell_de,
            self.frame_de_mean,
            self.frame_de_p99,
            self.frame_de_over_2 * 100.0,
            self.midtone_ev,
            self.clipped * 100.0,
            self.top_bin * 100.0,
            self.p999_ev,
            self.blown_chroma_rise
        )
    }
}

/// How far `now` moved from `reference`.
pub fn drift(reference: &Look, now: &Look) -> Drift {
    let mut drift = stats_drift(&reference.stats, &now.stats);
    let (cells, midtone_ev) = cell_drift(&reference.thumb, &now.thumb);
    if let Some(mut de) = cells {
        de.sort_by(f64::total_cmp);
        drift.cell_de = de.last().copied().unwrap_or(0.0);
        drift.frame_de_mean = de.iter().sum::<f64>() / de.len().max(1) as f64;
        drift.frame_de_p99 = percentile(&de, 0.99);
        drift.frame_de_over_2 = de.iter().filter(|d| **d > 2.0).count() as f64 / de.len() as f64;
        drift.midtone_ev = midtone_ev.abs();
    } else {
        // A thumbnail of another shape is a different picture: the crop or
        // the orientation changed.
        drift.cell_de = f64::INFINITY;
        drift.frame_de_mean = f64::INFINITY;
    }
    drift
}

/// The measures an anchor keeps: everything but the thumbnail's. The cells'
/// colour is unknown, and the mid-tones are read from the median instead.
pub fn stats_drift(reference: &Stats, now: &Stats) -> Drift {
    let ev = |a: f64, b: f64| {
        if a > 0.0 && b > 0.0 {
            (b / a).log2().abs()
        } else if a == b {
            0.0
        } else {
            f64::INFINITY
        }
    };
    let largest = |a: &[f64], b: &[f64]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    };
    Drift {
        midtone_ev: ev(reference.luma[P50], now.luma[P50]),
        clipped: largest(&reference.clipped, &now.clipped),
        top_bin: largest(&reference.top, &now.top),
        p999_ev: ev(reference.luma[P999], now.luma[P999]),
        blown_chroma_rise: (now.blown_chroma - reference.blown_chroma).max(0.0),
        ..Drift::default()
    }
}

/// Each cell's CIEDE2000, and the median log2 ratio of the mid-tone cells'
/// luminance (cells whose reference luminance is 0.05 to 0.8). None when the
/// thumbnails differ in shape.
fn cell_drift(reference: &Thumb, now: &Thumb) -> (Option<Vec<f64>>, f64) {
    if (reference.width, reference.height) != (now.width, now.height)
        || reference.cells.len() != now.cells.len()
    {
        return (None, 0.0);
    }
    let linear = |c: &[f32; 3]| c.map(srgb_decode);
    let mut de = Vec::with_capacity(reference.cells.len());
    let mut ratios = Vec::new();
    for (a, b) in reference.cells.iter().zip(&now.cells) {
        let (a, b) = (linear(a), linear(b));
        de.push(delta_e2000(lab(a), lab(b)));
        let (ya, yb) = (luma(a) as f64, luma(b) as f64);
        if (0.05..=0.8).contains(&ya) && yb > 0.0 {
            ratios.push((yb / ya).log2());
        }
    }
    ratios.sort_by(f64::total_cmp);
    let midtone = if ratios.is_empty() {
        0.0
    } else {
        percentile(&ratios, 0.5)
    };
    (Some(de), midtone)
}

/// The `q` quantile of sorted values.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i]
}

#[cfg(test)]
mod tests {
    use super::super::stats::measure;
    use super::*;
    use argentum_engine::colour::srgb_encode;
    use image::{Rgba, Rgba32FImage};

    /// A scene with what the check has to see: a grey ramp from black to
    /// past white, colours at three levels, and a blown sky, as a render
    /// hands it back (sRGB-encoded, clipped at white).
    fn scene() -> Rgba32FImage {
        let (w, h) = (640u32, 427u32);
        Rgba32FImage::from_fn(w, h, |x, y| {
            let t = x as f32 / (w - 1) as f32;
            let linear = match y * 4 / h {
                0 => [t * 1.3; 3],
                1 => [t, t * 0.55, t * 0.25],
                2 => [t * 0.2, t * 0.6, t * 0.9],
                _ if x > w * 2 / 3 => [1.4, 1.35, 1.3],
                _ => [0.18 + 0.5 * t, 0.2 + 0.45 * t, 0.25 + 0.4 * t],
            };
            let shown = linear.map(|v| srgb_encode(v.clamp(0.0, 1.0)));
            Rgba([shown[0], shown[1], shown[2], 1.0])
        })
    }

    fn passes(reference: &Rgba32FImage, now: &Rgba32FImage) -> bool {
        drift(&measure(reference), &measure(now))
            .breaches(&limits())
            .is_empty()
    }

    #[test]
    fn the_same_render_passes() {
        let s = scene();
        assert!(passes(&s, &s));
        assert_eq!(drift(&measure(&s), &measure(&s)), Drift::default());
    }

    /// The smallest change the check exists for: every value in the
    /// highlights one 8-bit level brighter.
    #[test]
    fn a_one_level_highlight_shift_fails() {
        let before = scene();
        let after = super::super::render_tier::one_level_brighter_highlights(&before);
        let d = drift(&measure(&before), &measure(&after));
        let breaches = d.breaches(&limits());
        assert!(
            !breaches.is_empty(),
            "a one-level highlight shift passed: {}",
            d.line()
        );
    }

    /// RapidRAW 1.6.7's Whites, as #171 took it, against the Whites it
    /// replaced, at +50 (a state the check renders): `apply_tonal_adjustments`
    /// in shaders/shader.wgsl with no local detail.
    fn whites(rgb: [f32; 3], amount: f32, (pivot, push, ramp_from): (f32, f32, f32)) -> [f32; 3] {
        let smoothstep = |e0: f32, e1: f32, x: f32| {
            let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let base = luma(rgb.map(|v| v.max(0.0))).max(0.0001);
        let t = base.powf(0.4545);
        let u = (t - pivot).max(0.0);
        let pushed = t + amount * push * (u * u) / (1.0 + u * u);
        let ramp = smoothstep(ramp_from, 0.9, t);
        let t_new = t + (pushed - t) * ramp;
        rgb.map(|v| v * t_new.powf(2.2) / base)
    }

    #[test]
    fn a_change_the_size_of_171s_whites_fails() {
        const BEFORE: (f32, f32, f32) = (0.45, 0.25, 0.25);
        const AFTER: (f32, f32, f32) = (0.4, 0.35, 0.1);
        let render = |params| {
            let s = scene();
            Rgba32FImage::from_fn(s.width(), s.height(), |x, y| {
                let p = s.get_pixel(x, y);
                let linear = [p[0], p[1], p[2]].map(srgb_decode);
                let out = whites(linear, 0.5, params).map(|v| srgb_encode(v.clamp(0.0, 1.0)));
                Rgba([out[0], out[1], out[2], 1.0])
            })
        };
        let d = drift(&measure(&render(BEFORE)), &measure(&render(AFTER)));
        assert!(
            !d.breaches(&limits()).is_empty(),
            "#171's Whites change passed: {}",
            d.line()
        );
    }

    #[test]
    fn noise_below_a_tenth_of_a_level_passes() {
        let before = scene();
        let mut after = before.clone();
        for (i, p) in after.pixels_mut().enumerate() {
            // A sign from a hash of the index, so no column moves as one.
            let up = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 63 == 0;
            let jitter = if up { 0.1 } else { -0.1 } / 255.0;
            for c in 0..3 {
                p[c] = (p[c] + jitter).clamp(0.0, 1.0);
            }
        }
        let d = drift(&measure(&before), &measure(&after));
        assert!(d.breaches(&limits()).is_empty(), "{}", d.line());
    }

    #[test]
    fn a_crop_is_a_different_picture() {
        let before = scene();
        let after = image::imageops::crop_imm(&before, 0, 0, 640, 300).to_image();
        assert!(!passes(&before, &after));
    }

    #[test]
    fn the_limits_are_never_below_the_boards() {
        let l = limits();
        assert!(l.cell_de >= BOARD.cell_de && l.p999_ev >= BOARD.p999_ev);
        assert!(l.midtone_ev >= BOARD.midtone_ev && l.top_bin >= BOARD.top_bin);
    }
}
