//! A camera's built-in lens correction: the per-shot tables it stores in the
//! RAW for the lens on the camera, at the focal length and aperture used.
//!
//! This file holds the model (`Radial`, `LensCorrection`, `Amounts`,
//! `LensMap`) and `read`, which asks each make's reader in turn. A make is
//! one file in `rm_lens_builtin/`, naming its own source, and one line in
//! `read`:
//!
//! - `fuji.rs`: the tables Fujifilm writes into every RAF, on by default.
//! - `dng.rs`: a DNG's lens opcodes, on by default.
//! - `sony.rs`: the tables Sony writes into an ARW, off by default.
//! - `olympus.rs`: the distortion and CA model Olympus and OM System write
//!   into every ORF, on by default.
//!
//! `fit.rs` holds what more than one reader shares. A reader whose radii
//! depend on the developed image's size asks for it (`developed_size_px`).
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `v0.4.0` @ a8c5d6d7 (MIT):
//! - `crates/rawmakase-model/src/optics/mod.rs`: `Radial`, `LensCorrection`,
//!   `radial_scale_with`, `fill_scale_with` and `fill_scale_within`, as
//!   written.
//! - `crates/rawmakase-engine/src/develop/image_space.rs`: `LensMap`'s
//!   position map, its Newton inverse, and which fill each source takes
//!   (RAWmakase #533).
//! - `docs/lens-corrections.md`: the order (vignetting gain in linear light
//!   at the source pixel, then distortion and CA as one radial remap while
//!   sampling, scaled so corrected corners stay inside the sensor).
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The amounts are the app's three Lens Correction sliders: distortion and
//!   vignetting as RAWmakase applies Lightroom's (the radial scale moved by
//!   the amount, the gain raised to it), and lateral CA moved the same way as
//!   distortion, where RAWmakase has no amount for it.
//! - `LensMap` works on offsets from the image centre in pixels, as the app's
//!   warp does, rather than on RAWmakase's image positions.
//! - Sony's fill keeps the developed image's edge inside it. rawler already
//!   crops an ARW to its DefaultCrop, which is the camera's aspect-ratio crop
//!   on a 16:9 ZV-1 shot, so the developed image is the frame RAWmakase's
//!   `camera_crop` describes; the sensor beyond it is not there to sample.

use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use serde::{Deserialize, Serialize};

mod dng;
mod fit;
mod fuji;
mod olympus;
mod sony;
#[cfg(test)]
mod test_files;

/// A radial function sampled at increasing radii. Radius 0 is the image
/// centre and 1 is half of the image diagonal. Values are linearly
/// interpolated and held constant beyond the outermost knots.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Radial {
    pub knots: Vec<f32>,
    pub values: Vec<f32>,
}

impl Radial {
    fn new(knots: Vec<f32>, values: Vec<f32>) -> Option<Self> {
        let r = Self { knots, values };
        r.valid().then_some(r)
    }

    fn valid(&self) -> bool {
        self.knots.len() >= 2
            && self.knots.len() == self.values.len()
            && self.knots.len() <= 64
            && self.knots.iter().chain(&self.values).all(|v| v.is_finite())
            && self.knots.windows(2).all(|k| k[0] < k[1])
            && self.knots[0] >= 0.
            && self.values.iter().all(|v| (0.2..=5.).contains(v))
    }

    pub fn eval(&self, r: f32) -> f32 {
        let k = &self.knots;
        let i = k.partition_point(|x| *x <= r);
        if i == 0 {
            return self.values[0];
        }
        if i == k.len() {
            return self.values[k.len() - 1];
        }
        let t = (r - k[i - 1]) / (k[i] - k[i - 1]);
        self.values[i - 1] + (self.values[i] - self.values[i - 1]) * t
    }
}

/// One flag for each part of a lens correction.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Parts {
    pub distortion: bool,
    pub chromatic: bool,
    pub vignetting: bool,
}

impl Parts {
    pub const ALL: Parts = Parts {
        distortion: true,
        chromatic: true,
        vignetting: true,
    };
}

/// Built-in lens correction for one photo.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LensCorrection {
    /// Where the data came from, for display, e.g. "Fujifilm built-in".
    pub source: String,
    /// Lightroom applies this correction by default. True where verified
    /// against Adobe's own rendering.
    #[serde(default)]
    pub default_on: bool,
    /// Gain that restores illumination at each radius (1 = none).
    pub vignetting: Option<Radial>,
    /// Source radius divided by output radius for the green channel (1 = none).
    pub distortion: Option<Radial>,
    /// Additional source radius scale for red and blue relative to green (1 = none).
    pub chromatic: Option<[Radial; 2]>,
    /// The parts the camera always corrects, as Lightroom applies them from
    /// the file whatever its profile setting. A lens profile corrects the
    /// other parts, and the camera's own tables for them only stand in where
    /// the profile has none. `None`, as in every correction stored before this
    /// field: all three, so no profile is ever used beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always: Option<Parts>,
}

/// How much of each part to apply, 1 = all of it: the app's Lens Correction
/// amounts (0 to 200%) over 100. An amount of 0 turns that part off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Amounts {
    pub distortion: f32,
    pub chromatic: f32,
    pub vignetting: f32,
}

impl Amounts {
    pub const FULL: Amounts = Amounts {
        distortion: 1.,
        chromatic: 1.,
        vignetting: 1.,
    };
}

impl LensCorrection {
    /// The `source` of the correction read from a Sony file's own tables.
    pub const SONY_BUILTIN: &'static str = "Sony built-in";

    pub fn is_empty(&self) -> bool {
        self.vignetting.is_none() && self.distortion.is_none() && self.chromatic.is_none()
    }

    /// The parts this correction holds data for: for a camera whose stored
    /// correction Lightroom applies whatever its profile setting, the parts it
    /// always corrects (`always`).
    fn held_parts(&self) -> Parts {
        Parts {
            distortion: self.distortion.is_some(),
            chromatic: self.chromatic.is_some(),
            vignetting: self.vignetting.is_some(),
        }
    }

    pub fn validate(&self) -> bool {
        self.vignetting.iter().all(Radial::valid)
            && self.distortion.iter().all(Radial::valid)
            && self.chromatic.iter().flatten().all(Radial::valid)
    }

    /// Source radius scale for red, green and blue at output radius `r`.
    pub fn radial_scale_with(&self, r: f32, amounts: Amounts) -> [f32; 3] {
        let g = self
            .distortion
            .as_ref()
            .map_or(1., |d| 1. + (d.eval(r) - 1.) * amounts.distortion);
        let ca = |c: &Radial| 1. + (c.eval(r) - 1.) * amounts.chromatic;
        match &self.chromatic {
            Some([red, blue]) => [g * ca(red), g, g * ca(blue)],
            None => [g; 3],
        }
    }

    /// Output radii are scaled by this factor so every corrected corner
    /// samples inside the sensor, as Lightroom crops the undefined border
    /// after distortion correction.
    pub fn fill_scale_with(&self, amounts: Amounts) -> f32 {
        if self.distortion.is_none() && self.chromatic.is_none() {
            return 1.;
        }
        let widest = (0..=64)
            .flat_map(|i| self.radial_scale_with(i as f32 / 64., amounts))
            .fold(1f32, f32::max);
        1. / widest
    }

    /// As `fill_scale_with`, for a photo that is only the rectangle `crop`
    /// (`[left, top, right, bottom]`, in pixels) of the decoded image of
    /// `size`: the scale keeps every corrected sample of the crop's edge inside
    /// the decoded image, which can reach beyond the crop (a camera's
    /// aspect-ratio crop of a wider sensor). Never below the scale for the
    /// whole image.
    pub fn fill_scale_within(&self, amounts: Amounts, size: [f32; 2], crop: [f32; 4]) -> f32 {
        let whole = self.fill_scale_with(amounts);
        if whole >= 1. {
            return whole;
        }
        let [w, h] = size;
        let half = w.hypot(h) * 0.5;
        let [left, top, right, bottom] = crop.map(|v| v.max(0.));
        let (x0, x1) = (left - w * 0.5, right.min(w) - w * 0.5);
        let (y0, y1) = (top - h * 0.5, bottom.min(h) - h * 0.5);
        let inside = |dx: f32, dy: f32, fill: f32| {
            let s = self
                .radial_scale_with(dx.hypot(dy) * fill / half, amounts)
                .into_iter()
                .fold(0f32, f32::max);
            dx.abs() * fill * s <= w * 0.5 && dy.abs() * fill * s <= h * 0.5
        };
        // About one sample per pixel along the edge, so no extremum between
        // samples leaves the image by more than a fraction of a pixel.
        let steps = (w.max(h).ceil() as usize).clamp(64, 8192);
        let mut fill = 1f32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            for (dx, dy) in [(x, y0), (x, y1), (x0, y), (x1, y)] {
                if inside(dx, dy, fill) {
                    continue;
                }
                let (mut lo, mut hi) = (whole, fill);
                for _ in 0..24 {
                    let mid = (lo + hi) * 0.5;
                    if inside(dx, dy, mid) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                fill = lo;
            }
        }
        fill
    }

    /// The gain that restores the light at radius `r` of the decoded image.
    pub fn vignetting_gain(&self, r: f32, amount: f32) -> f32 {
        match &self.vignetting {
            None => 1.,
            Some(v) if amount == 1. => v.eval(r),
            Some(v) => v.eval(r).powf(amount),
        }
    }
}

/// The correction as a map of positions over one image: from an output
/// position to where each channel samples the image, and back. Positions are
/// offsets from the image centre in pixels.
#[derive(Clone, Copy, Debug)]
pub struct LensMap<'a> {
    pub lens: &'a LensCorrection,
    pub amounts: Amounts,
    /// Half the image diagonal, in pixels: radius 1.
    pub half: f32,
    /// The scale that keeps corrected samples inside the image.
    pub fill: f32,
}

impl<'a> LensMap<'a> {
    pub fn new(lens: &'a LensCorrection, width: f32, height: f32, amounts: Amounts) -> Self {
        Self {
            lens,
            amounts,
            half: width.hypot(height) * 0.5,
            fill: Self::fill(lens, width, height, amounts),
        }
    }

    /// The scale that keeps corrected samples inside the image. Sony's
    /// built-in correction is the camera's own, which zooms by no more than
    /// its table does, so only the photo's edge has to stay inside. Any other
    /// correction keeps the whole frame's corners inside, as Lightroom's
    /// profiles do.
    fn fill(lens: &LensCorrection, width: f32, height: f32, amounts: Amounts) -> f32 {
        if lens.source == LensCorrection::SONY_BUILTIN {
            lens.fill_scale_within(amounts, [width, height], [0., 0., width, height])
        } else {
            lens.fill_scale_with(amounts)
        }
    }

    /// Output offset (`dx`, `dy`) scaled by the fill, and the per-channel
    /// radial scale there: red, green and blue sample at the offset times
    /// their scale.
    pub fn scales(&self, dx: f32, dy: f32) -> ([f32; 2], [f32; 3]) {
        let (dx, dy) = (dx * self.fill, dy * self.fill);
        let r = dx.hypot(dy) / self.half;
        ([dx, dy], self.lens.radial_scale_with(r, self.amounts))
    }

    /// Where green samples for output offset (`dx`, `dy`).
    pub fn forward(&self, dx: f32, dy: f32) -> [f32; 2] {
        let ([dx, dy], scale) = self.scales(dx, dy);
        [dx * scale[1], dy * scale[1]]
    }

    /// The vignetting gain for the image pixel at offset (`dx`, `dy`).
    pub fn gain(&self, dx: f32, dy: f32) -> f32 {
        self.lens
            .vignetting_gain(dx.hypot(dy) / self.half, self.amounts.vignetting)
    }

    /// The output offset whose green samples image offset (`dx`, `dy`): the
    /// radius is found with a few Newton steps, the direction is kept.
    pub fn inverse(&self, dx: f32, dy: f32) -> [f32; 2] {
        let target = dx.hypot(dy);
        if target < 1e-6 {
            return [dx, dy];
        }
        let f = |rho: f32| rho * self.lens.radial_scale_with(rho / self.half, self.amounts)[1];
        let mut rho = target;
        for _ in 0..8 {
            let h = self.half * 1e-3;
            let slope = (f(rho + h) - f(rho - h)) / (2. * h);
            if slope.abs() < 1e-6 {
                break;
            }
            let step = (f(rho) - target) / slope;
            rho = (rho - step).max(0.);
            if step.abs() < 1e-4 {
                break;
            }
        }
        let k = rho / target / self.fill;
        [dx * k, dy * k]
    }
}

/// The built-in correction a RAW carries. `None` for a file of another
/// format, one that stored no correction, or tables that do not parse.
/// One line per make, asked in this order; each reader answers only for
/// its own format.
pub fn read(file: &[u8]) -> Option<LensCorrection> {
    read_sized(file, &|| developed_size_px(file))
}

/// `read`, the developed image's width and height in pixels coming from
/// `size_px`, which only a reader whose radii depend on them calls: the
/// file's own (`developed_size_px`) in the app, a test file's in the tests.
fn read_sized(file: &[u8], size_px: &dyn Fn() -> Option<[f64; 2]>) -> Option<LensCorrection> {
    let c = fuji::read(file)
        .or_else(|| dng::read(file))
        .or_else(|| sony::read(file))
        .or_else(|| olympus::read(file, size_px))?;
    (!c.is_empty() && c.validate()).then_some(c)
}

/// The width and height in pixels of the image `develop` makes of `file`,
/// the image the radii are fractions of: rawler's geometry alone, from a
/// dummy decode, which reads no pixel data. `None` for a file rawler does
/// not open.
fn developed_size_px(file: &[u8]) -> Option<[f64; 2]> {
    let source = RawSource::new_from_slice(file);
    let raw = rawler::get_decoder(&source)
        .ok()?
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    let (w, h) = crate::develop::developed_size_px(&raw);
    (w > 0 && h > 0).then_some([w as f64, h as f64])
}

/// A TIFF's first bytes, little- or big-endian: how a DNG and an ARW begin.
fn is_tiff(file: &[u8]) -> bool {
    file.starts_with(b"II*\0") || file.starts_with(b"MM\0*")
}

#[cfg(test)]
mod tests {
    use super::test_files::{
        ZV1_CA, ZV1_DIST, ZV1_VIG, digest, dji_file, fuji_file, raf, sony_file,
    };
    use super::*;

    #[test]
    fn radial_interpolates_and_clamps() {
        let r = Radial::new(vec![0., 0.5, 1.], vec![1., 1.2, 1.6]).unwrap();
        assert_eq!(r.eval(-1.), 1.);
        assert!((r.eval(0.25) - 1.1).abs() < 1e-6);
        assert!((r.eval(0.75) - 1.4).abs() < 1e-6);
        assert_eq!(r.eval(2.), 1.6);
        assert!(Radial::new(vec![0., 0.], vec![1., 1.]).is_none());
        assert!(Radial::new(vec![0., 1.], vec![1., f32::NAN]).is_none());
    }

    #[test]
    fn fill_scale_keeps_corners_inside() {
        let c = LensCorrection {
            distortion: Radial::new(vec![0., 1.], vec![1., 1.02]),
            ..Default::default()
        };
        let s = c.fill_scale_with(Amounts::FULL);
        assert!(s < 1. && s > 0.97);
        assert!(c.radial_scale_with(s, Amounts::FULL)[1] * s <= 1.0001);
        assert_eq!(LensCorrection::default().fill_scale_with(Amounts::FULL), 1.);
    }

    #[test]
    fn amounts_scale_each_part() {
        let c = LensCorrection {
            distortion: Radial::new(vec![0., 1.], vec![1., 1.1]),
            chromatic: Some([
                Radial::new(vec![0., 1.], vec![1., 1.01]).unwrap(),
                Radial::new(vec![0., 1.], vec![1., 0.99]).unwrap(),
            ]),
            vignetting: Radial::new(vec![0., 1.], vec![1., 2.]),
            ..Default::default()
        };
        let half = Amounts {
            distortion: 0.5,
            chromatic: 0.,
            vignetting: 2.,
        };
        let [r, g, b] = c.radial_scale_with(1., half);
        assert!((g - 1.05).abs() < 1e-6);
        assert!(r == g && b == g, "no CA at amount 0");
        assert!((c.vignetting_gain(1., 2.) - 4.).abs() < 1e-5);
        assert_eq!(c.vignetting_gain(1., 0.), 1.);
    }

    #[test]
    fn lens_inverse_undoes_forward() {
        let lens = LensCorrection {
            distortion: Radial::new(vec![0., 0.5, 1.], vec![1., 1.02, 1.08]),
            ..Default::default()
        };
        let map = LensMap::new(&lens, 300., 200., Amounts::FULL);
        assert!(map.fill < 1.);
        for (x, y) in [(-140., -80.), (0., 0.), (140., 90.), (-110., 70.)] {
            let [gx, gy] = map.forward(x, y);
            let [bx, by] = map.inverse(gx, gy);
            assert!((bx - x).abs() < 1e-2 && (by - y).abs() < 1e-2, "{x} {y}");
        }
    }

    #[test]
    fn unsupported_or_truncated_files_have_no_correction() {
        let mut header_only = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
        header_only.resize(104, 0xff);
        for (name, bytes) in [
            ("empty", &b""[..]),
            ("text", &[b'x'; 200][..]),
            ("tiff", b"II*\0\x08\0\0\0\0\0"),
            ("raf header pointing nowhere", &header_only[..]),
        ] {
            assert!(read(bytes).is_none(), "{name}");
        }
        // A RAF whose FujiIFD has no tables.
        assert!(read(&raf(vec![(0xf001, 4, 1, vec![1, 0, 0, 0])])).is_none());
    }

    #[test]
    fn fill_within_a_crop_of_a_wider_sensor_is_looser_and_still_inside() {
        let c = LensCorrection {
            distortion: Radial::new(vec![0., 1.], vec![1., 1.08]),
            ..Default::default()
        };
        let size = [6000., 4000.];
        // A 16:9 crop of the 3:2 image.
        let crop = [0., 437.5, 6000., 3812.5];
        let whole = c.fill_scale_with(Amounts::FULL);
        let within = c.fill_scale_within(Amounts::FULL, size, crop);
        assert!(within > whole + 0.005 && within <= 1., "{whole} {within}");
        let half = 6000f32.hypot(4000.) * 0.5;
        for (x, y) in [
            (3000f32, 1562.5f32),
            (3000., 0.),
            (0., 1562.5),
            (1500., 1562.5),
        ] {
            let s = c.radial_scale_with(x.hypot(y) * within / half, Amounts::FULL)[1];
            assert!(
                x * within * s <= 3000.01 && y * within * s <= 2000.01,
                "{x} {y}"
            );
        }
        // The whole image is never tighter than its own scale; no distortion needs none.
        let all = c.fill_scale_within(Amounts::FULL, size, [0., 0., 6000., 4000.]);
        assert!(all >= whole && all < whole + 0.01, "{whole} {all}");
        assert_eq!(
            LensCorrection::default().fill_scale_within(Amounts::FULL, size, crop),
            1.
        );
    }

    #[test]
    fn fill_within_leaves_no_edge_pixel_outside_between_samples() {
        // A table with bumps, the kind Sony's signed values give.
        let c = LensCorrection {
            distortion: Radial::new(
                (0..11).map(|i| i as f32 / 10.).collect(),
                vec![
                    1.08, 1.06, 1.04, 1.02, 1.02, 1.02, 1.02, 1.02, 1.02, 1.03, 0.99,
                ],
            ),
            ..Default::default()
        };
        let (w, h) = (6000f32, 4000f32);
        let fill = c.fill_scale_within(Amounts::FULL, [w, h], [0., 0., w, h]);
        let half = w.hypot(h) * 0.5;
        let mut worst = 0f32;
        for i in 0..6000 {
            for (px, py) in [(i as f32 + 0.5, 0.5), (i as f32 + 0.5, h - 0.5)] {
                let (dx, dy) = (px - w * 0.5, py - h * 0.5);
                let s = c.radial_scale_with(dx.hypot(dy) * fill / half, Amounts::FULL)[1];
                worst = worst.max((dx * fill * s).abs() - w * 0.5);
                worst = worst.max((dy * fill * s).abs() - h * 0.5);
            }
        }
        assert!(worst < 0.5, "{worst}");
    }

    #[test]
    fn sonys_tables_keep_the_photos_frame_others_its_corners() {
        // The ZV-1's distortion: 1.077 at the centre, 0.895 at the corner.
        let sony = read(&sony_file(&ZV1_VIG, &ZV1_CA, &ZV1_DIST)).unwrap();
        let (w, h) = (5472f32, 3648f32);
        let map = LensMap::new(&sony, w, h, Amounts::FULL);
        let within = sony.fill_scale_within(Amounts::FULL, [w, h], [0., 0., w, h]);
        assert_eq!(map.fill, within);
        assert!(
            map.fill > sony.fill_scale_with(Amounts::FULL) + 0.01,
            "{}",
            map.fill
        );
        // The same tables from anywhere else keep the whole frame's corners inside.
        let other = LensCorrection {
            source: "DNG".into(),
            ..sony.clone()
        };
        assert_eq!(
            LensMap::new(&other, w, h, Amounts::FULL).fill,
            other.fill_scale_with(Amounts::FULL)
        );
    }

    /// A real RAW with a built-in correction (AG_RAW): its tables at a few
    /// radii, and the zoom that keeps the corners inside on a 3:2 frame.
    /// Prints; asserts only that one was read.
    #[test]
    #[ignore = "reads a local RAW named by AG_RAW; run by hand"]
    fn a_real_files_tables() {
        let path = std::env::var("AG_RAW").expect("AG_RAW");
        let c = read(&std::fs::read(&path).expect("read")).expect("a correction");
        let at = |r: &Option<Radial>| -> Vec<f32> {
            r.as_ref()
                .map_or(vec![], |r| [0., 0.5, 0.8, 1.].map(|x| r.eval(x)).to_vec())
        };
        eprintln!();
        eprintln!("{path}: {}", c.source);
        eprintln!(
            "  vignetting gain at r 0/0.5/0.8/1: {:.3?}",
            at(&c.vignetting)
        );
        eprintln!("  distortion at r 0/0.5/0.8/1: {:.4?}", at(&c.distortion));
        if let Some([red, blue]) = &c.chromatic {
            eprintln!(
                "  CA red/blue at r 1: {:.5} {:.5}",
                red.eval(1.),
                blue.eval(1.)
            );
        }
        let map = LensMap::new(&c, 6000., 4000., Amounts::FULL);
        eprintln!(
            "  fill on a 3:2 frame {:.4} (whole-frame corners {:.4})",
            map.fill,
            c.fill_scale_with(Amounts::FULL)
        );
    }

    /// Today's readers, pinned: what each test file reads as. A change meant
    /// to move a reader updates its line here; any other change fails.
    #[test]
    fn todays_readers_are_pinned() {
        for (name, file, want) in [
            ("fujifilm", fuji_file(), 9_132_833_245_852_271_612),
            ("dji dng", dji_file(), 17_364_010_675_930_012_272),
            (
                "zv-1",
                sony_file(&ZV1_VIG, &ZV1_CA, &ZV1_DIST),
                16_060_793_216_672_783_479,
            ),
        ] {
            assert_eq!(digest(&read(&file)), want, "{name}");
        }
    }
}
