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
//! - `sony.rs`: the tables Sony writes into an ARW, on by default for the
//!   parts Lightroom applies (lateral CA, and a compact's distortion).
//! - `nikon.rs`: the model a Nikon Z writes into a NEF's NEFInfo, on by
//!   default for a lens that needs it, off for one whose correction is
//!   optional; and the tables a Coolpix P950, P1000 or P1100 writes there,
//!   on by default.
//! - `olympus.rs`: the distortion and CA model Olympus and OM System write
//!   into every ORF, on by default.
//! - `panasonic.rs`: the distortion and CA corrections Panasonic writes into
//!   every RW2, on by default.
//! - `samsung.rs`: the distortion and CA models Samsung NX bodies write into
//!   an SRW, on by default.
//! - `canon.rs`: the tables an EOS R5 Mark II or R6 Mark III writes into a
//!   CR3, off by default.
//!
//! `fit.rs` holds the maths and helpers more than one reader shares. A reader
//! whose radii or optical centre depend on where the developed image lies
//! asks for it (`developed_frame`).
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
//! RAWmakase `52da1bf` (v0.4.6, MIT), the same files: the optical centre
//! (`LensCorrection::centre`, `reach`) that every part of the correction is
//! measured from, and Lightroom's framing (`fill_scale`): zoom only as far
//! as green's samples for the photo's edge need to stay inside, so neither a
//! ratio above 1 inside the frame nor red and blue reaching past the corner
//! zooms in. Their tests on both, and on `LensMap` about an off-centre axis.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The amounts are the app's three Lens Correction sliders: distortion and
//!   vignetting as RAWmakase applies Lightroom's (the radial scale moved by
//!   the amount, the gain raised to it), and lateral CA moved the same way as
//!   distortion, where RAWmakase has no amount for it.
//! - `LensMap` works on offsets from the image centre in pixels, as the app's
//!   warp does, rather than on RAWmakase's image positions; the optical
//!   centre is one such offset (`LensMap::centre`).
//! - Sony's fill keeps the developed image's edge inside it. rawler already
//!   crops an ARW to its DefaultCrop, which is the camera's aspect-ratio crop
//!   on a 16:9 ZV-1 shot, so the developed image is the frame RAWmakase's
//!   `camera_crop` describes; the sensor beyond it is not there to sample.
//! - The same holds for every make since 52da1bf, whose fill keeps the
//!   default crop's edge inside the decoded image: here the developed image
//!   is the default crop, so `fill_scale` keeps its own edge inside and
//!   takes no frame. RAWmakase centres Olympus, Panasonic and Samsung on the
//!   default crop's centre, which is the developed image's centre here.
//! - RAWmakase measures Fujifilm's, Sony's and Canon's tables in its decoded
//!   image, centred on it. rawler's decoded image is another for those
//!   bodies (an A7 IV's has no active area, so it keeps masked columns; an
//!   X-T30 III's is marked TODO; an R5 Mark II's has a bug hint), so moving
//!   the tables from it put the centre 22 px off RAWmakase's on the golden
//!   A7 IV. Here they stay centred on the developed image, radius 1 at its
//!   half diagonal: within 2.2 px of RAWmakase's centre on the golden Sony
//!   and Canon photos, 6 px on the X-T30 III (`look_check/lens_baseline.rs`
//!   holds those numbers).
//! - The centre is an `Option`: a correction stored in an edit before it was
//!   read has none, and keeps its centre and framing then (`fill_scale_with`,
//!   Sony's `fill_scale_within`), so no saved photo moves. RAWmakase gives
//!   such data the image centre and frames it the new way.
//! - Since RAWmakase `44b21d5` a reader there can give two corrections
//!   (`lens::embedded::Builtin`): what Lightroom applies from the file
//!   whatever its profile setting, and the camera's fuller data standing in
//!   for a profile. Here they are one correction, its `always` naming the
//!   first (`LensCorrection::always_correcting`).

use serde::{Deserialize, Serialize};

pub use crate::develop::DevelopedFrame;

mod canon;
mod dng;
mod fit;
mod fuji;
mod nikon;
mod olympus;
mod panasonic;
mod samsung;
mod sony;
#[cfg(test)]
mod test_files;

/// A radial function sampled at increasing radii. Radius 0 is the
/// correction's optical centre (`LensCorrection::centre`) and 1 is half of
/// the image diagonal, so an off-centre correction's farthest corner lies
/// beyond 1 (`reach`). Values are linearly interpolated and held constant
/// beyond the outermost knots.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    #[cfg_attr(feature = "ts", ts(optional))]
    pub always: Option<Parts>,
    /// The optical centre the radial functions are measured from, in
    /// fractions of the developed image's width and height: where the
    /// camera's data places it, the image centre where it records none
    /// (`read` fills that in). `None`, as in every correction stored before
    /// this field: the image centre, framed as it was then
    /// (`fill_scale_with`), so a saved edit renders as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub centre: Option<[f32; 2]>,
}

/// The distance from `centre` (fractions of `size`'s width and height) to the
/// farthest corner of an image of `size` pixels, in units of its half
/// diagonal: 1 for the image centre, more off it.
// RAWmakase 52da1bf: crates/rawmakase-model/src/optics/mod.rs
pub fn reach(centre: [f32; 2], size: [f32; 2]) -> f32 {
    let [w, h] = size;
    let x = centre[0].max(1. - centre[0]) * w;
    let y = centre[1].max(1. - centre[1]) * h;
    if w > 0. && h > 0. {
        x.hypot(y) / (w.hypot(h) * 0.5)
    } else {
        1.
    }
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

    /// The centre of the image, in fractions of its width and height.
    pub const IMAGE_CENTRE: [f32; 2] = [0.5, 0.5];

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
        self.centre.iter().flatten().all(|c| (0. ..=1.).contains(c))
            && self.vignetting.iter().all(Radial::valid)
            && self.distortion.iter().all(Radial::valid)
            && self.chromatic.iter().flatten().all(Radial::valid)
    }

    /// The correction with the camera always correcting those of `parts` it
    /// has tables for, as Lightroom applies them from the file: on by
    /// default, its other tables standing in for a profile. With none of
    /// them, the whole correction only stands in for a profile: off by
    /// default, and correcting all of it once switched on.
    fn always_correcting(self, parts: Parts) -> Self {
        let always = Parts {
            distortion: parts.distortion && self.distortion.is_some(),
            chromatic: parts.chromatic && self.chromatic.is_some(),
            vignetting: parts.vignetting && self.vignetting.is_some(),
        };
        let any = always.distortion || always.chromatic || always.vignetting;
        Self {
            default_on: any,
            always: any.then_some(always),
            ..self
        }
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
    /// samples inside the sensor, in every channel: the framing of a
    /// correction stored before its optical centre was read (`centre` is
    /// `None`), kept for the edits saved with it. A correction read now is
    /// framed by `fill_scale`.
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
    /// whole image. Sony's framing before the optical centre was read.
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

    /// The scale output offsets from the optical centre take so that green's
    /// sample for every point of the edge of the developed image of `size`
    /// pixels, which is the photo's (the camera's default crop), lies inside
    /// it, with the Distortion amount: as Lightroom frames the corrected
    /// photo, never zooming out. Only green frames it, as in Adobe's warps,
    /// whose red and blue planes may reach a little past the image's edge;
    /// the sampler clamps them there. Where the corrected radius grows with
    /// the output radius, as it does for any real lens, the interior samples
    /// nearer the centre than the edge along the same ray, so it stays inside
    /// too; a correction that folds back keeps every radius out to the
    /// farthest corner inside instead.
    // RAWmakase 52da1bf: crates/rawmakase-model/src/optics/mod.rs
    pub fn fill_scale(&self, amounts: Amounts, size: [f32; 2]) -> f32 {
        let Some(distortion) = &self.distortion else {
            return 1.;
        };
        let [w, h] = size;
        let half = w.hypot(h) * 0.5;
        if half.is_nan() || half <= 0. {
            return 1.;
        }
        let green = |r: f32| 1. + (distortion.eval(r) - 1.) * amounts.distortion;
        let centre = self.centre.unwrap_or(Self::IMAGE_CENTRE);
        let reach = reach(centre, size);
        // Every radius out to the farthest corner sampling at or inside its own keeps
        // every sample inside the image, which is convex around the centre.
        let radii = (0..=256).map(|i| reach * i as f32 / 256.);
        let whole = 1. / radii.clone().map(green).fold(1f32, f32::max);
        let mut previous = 0f32;
        let monotonic = radii.skip(1).all(|r| {
            let s = r * green(r);
            let rising = s > previous;
            previous = s;
            rising
        });
        if !monotonic {
            return whole;
        }
        let [cx, cy] = [centre[0] * w, centre[1] * h];
        let (x0, x1) = (-cx, w - cx);
        let (y0, y1) = (-cy, h - cy);
        let inside = |dx: f32, dy: f32, fill: f32| {
            let s = fill * green(dx.hypot(dy) * fill / half);
            (0. ..=w).contains(&(cx + dx * s)) && (0. ..=h).contains(&(cy + dy * s))
        };
        // About one sample per pixel along the edge, so no extremum between samples
        // leaves the image by more than a fraction of a pixel.
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

    /// The gain that restores the light at radius `r` from the optical centre.
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
/// offsets from the image centre in pixels; the correction is measured from
/// its optical centre, `centre` pixels from there.
#[derive(Clone, Copy, Debug)]
pub struct LensMap<'a> {
    pub lens: &'a LensCorrection,
    pub amounts: Amounts,
    /// The optical centre, as an offset from the image centre in pixels.
    pub centre: [f32; 2],
    /// Half the image diagonal, in pixels: radius 1.
    pub half: f32,
    /// The scale that keeps corrected samples inside the image.
    pub fill: f32,
}

impl<'a> LensMap<'a> {
    pub fn new(lens: &'a LensCorrection, width: f32, height: f32, amounts: Amounts) -> Self {
        let [cx, cy] = lens.centre.unwrap_or(LensCorrection::IMAGE_CENTRE);
        Self {
            lens,
            amounts,
            centre: [(cx - 0.5) * width, (cy - 0.5) * height],
            half: width.hypot(height) * 0.5,
            fill: Self::fill(lens, width, height, amounts),
        }
    }

    /// The scale that keeps corrected samples inside the image. A correction
    /// read with its optical centre keeps green's samples for the photo's
    /// edge inside, as Lightroom frames it (`fill_scale`): the developed
    /// image is the camera's default crop, so its edge is the photo's. One
    /// stored before keeps its framing then: Sony's built-in correction only
    /// the photo's edge, in every channel; any other the whole frame's
    /// corners (`fill_scale_with`).
    fn fill(lens: &LensCorrection, width: f32, height: f32, amounts: Amounts) -> f32 {
        if lens.centre.is_some() {
            lens.fill_scale(amounts, [width, height])
        } else if lens.source == LensCorrection::SONY_BUILTIN {
            lens.fill_scale_within(amounts, [width, height], [0., 0., width, height])
        } else {
            lens.fill_scale_with(amounts)
        }
    }

    /// Output offset (`dx`, `dy`) from the optical centre, scaled by the
    /// fill, and the per-channel radial scale there: red, green and blue
    /// sample at the optical centre plus the offset times their scale.
    pub fn scales(&self, dx: f32, dy: f32) -> ([f32; 2], [f32; 3]) {
        let dx = (dx - self.centre[0]) * self.fill;
        let dy = (dy - self.centre[1]) * self.fill;
        let r = dx.hypot(dy) / self.half;
        ([dx, dy], self.lens.radial_scale_with(r, self.amounts))
    }

    /// Where green samples for output offset (`dx`, `dy`).
    pub fn forward(&self, dx: f32, dy: f32) -> [f32; 2] {
        let ([dx, dy], scale) = self.scales(dx, dy);
        [
            self.centre[0] + dx * scale[1],
            self.centre[1] + dy * scale[1],
        ]
    }

    /// The vignetting gain for the image pixel at offset (`dx`, `dy`).
    pub fn gain(&self, dx: f32, dy: f32) -> f32 {
        let r = (dx - self.centre[0]).hypot(dy - self.centre[1]) / self.half;
        self.lens.vignetting_gain(r, self.amounts.vignetting)
    }

    /// The output offset whose green samples image offset (`dx`, `dy`): the
    /// radius from the optical centre is found with a few Newton steps, the
    /// direction is kept.
    pub fn inverse(&self, dx: f32, dy: f32) -> [f32; 2] {
        let [ox, oy] = self.centre;
        let (dx, dy) = (dx - ox, dy - oy);
        let target = dx.hypot(dy);
        if target < 1e-6 {
            return [ox + dx, oy + dy];
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
        [ox + dx * k, oy + dy * k]
    }
}

/// The built-in correction a RAW carries. `None` for a file of another
/// format, one that stored no correction, or tables that do not parse.
/// One line per make, asked in this order; each reader answers only for
/// its own format. Nikon's and Samsung's ask the file's Make before Sony's,
/// which reads any other TIFF's SubIFD.
pub fn read(file: &[u8]) -> Option<LensCorrection> {
    read_with_frame(file, &|| developed_frame(file))
}

/// `read`, where the developed image lies coming from `frame`, which only a
/// reader whose radii or optical centre depend on it calls: the file's own
/// (`developed_frame`) in the app, a test file's in the tests. A reader
/// that records no optical centre is centred on the image, which is the
/// camera's default crop, as Adobe centres those makes' warps.
fn read_with_frame(
    file: &[u8],
    frame: &dyn Fn() -> Option<DevelopedFrame>,
) -> Option<LensCorrection> {
    // Read once, by the first reader that asks.
    let once = std::cell::OnceCell::new();
    let frame = || *once.get_or_init(frame);
    let size_px = || frame().map(|f| [f.crop[2], f.crop[3]].map(|v| v as f64));
    let mut c = fuji::read(file)
        .or_else(|| dng::read(file, &frame))
        .or_else(|| nikon::read(file, &frame))
        .or_else(|| samsung::read(file, &size_px))
        .or_else(|| sony::read(file))
        .or_else(|| olympus::read(file, &size_px))
        .or_else(|| panasonic::read(file, &size_px))
        .or_else(|| canon::read(file, &size_px))?;
    c.centre.get_or_insert(LensCorrection::IMAGE_CENTRE);
    (!c.is_empty() && c.validate()).then_some(c)
}

/// A TIFF's first bytes, little- or big-endian: how a DNG, an ARW, a NEF
/// and an SRW begin.
fn is_tiff(file: &[u8]) -> bool {
    file.starts_with(b"II*\0") || file.starts_with(b"MM\0*")
}

/// SubIFDs, in IFD0: an ARW's and a NEF's raw image directory is one.
const SUB_IFDS: u16 = 0x14a;

/// Where the image `develop` makes of `file` lies: rawler's raw image at
/// its default crop, inside the image demosaic makes
/// (`develop::developed_frame_px`), read from the file's geometry without
/// decoding a pixel: the image every reader's radii and centre are in.
/// `None` for a file rawler does not read.
pub fn developed_frame(file: &[u8]) -> Option<DevelopedFrame> {
    use rawler::decoders::RawDecodeParams;
    use rawler::rawsource::RawSource;

    let source = RawSource::new_from_slice(file);
    // The loader's decode, which turns a decoder's panic into an error.
    let raw = (rawler::global_loader())
        .decode(&source, &RawDecodeParams::default(), true)
        .ok()?;
    Some(crate::develop::developed_frame_px(&raw))
}

#[cfg(test)]
mod tests {
    use super::test_files::{
        P1100, Z7, Z8, ZV1_CA, ZV1_DIST, ZV1_VIG, coolpix_file, digest, dji_file, fuji_file,
        nikon_file, raf, sony_file,
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

    // RAWmakase 52da1bf: crates/rawmakase-model/src/optics/mod.rs and
    // crates/rawmakase-engine/src/develop/image_space.rs, their tests, on
    // `fill_scale` and `LensMap` here.

    /// Green's fill for a read correction over an image of `size`.
    fn green_fill(c: &LensCorrection, size: [f32; 2]) -> f32 {
        c.fill_scale(Amounts::FULL, size)
    }

    /// How far green's sample for any pixel of the image's edge lies outside it.
    fn worst_outside(c: &LensCorrection, size: [f32; 2], fill: f32) -> f32 {
        let [w, h] = size;
        let [cx, cy] = c.centre.unwrap_or(LensCorrection::IMAGE_CENTRE);
        let [cx, cy] = [cx * w, cy * h];
        let half = w.hypot(h) * 0.5;
        let mut worst = f32::NEG_INFINITY;
        let edge = (0..w as usize)
            .flat_map(|i| [(i as f32 + 0.5, 0.), (i as f32 + 0.5, h)])
            .chain((0..h as usize).flat_map(|i| [(0., i as f32 + 0.5), (w, i as f32 + 0.5)]));
        for (px, py) in edge {
            let (dx, dy) = (px - cx, py - cy);
            let s = fill * c.radial_scale_with(dx.hypot(dy) * fill / half, Amounts::FULL)[1];
            let (x, y) = (cx + dx * s, cy + dy * s);
            worst = worst.max(-x).max(x - w).max(-y).max(y - h);
        }
        worst
    }

    fn read_now(c: LensCorrection) -> LensCorrection {
        LensCorrection {
            centre: Some(c.centre.unwrap_or(LensCorrection::IMAGE_CENTRE)),
            ..c
        }
    }

    #[test]
    fn green_fill_keeps_the_edge_inside() {
        let c = read_now(LensCorrection {
            distortion: Radial::new(vec![0., 1.], vec![1., 1.02]),
            ..Default::default()
        });
        let size = [300., 200.];
        let s = green_fill(&c, size);
        assert!(s < 1. && s > 0.97);
        assert!(c.radial_scale_with(s, Amounts::FULL)[1] * s <= 1.0001);
        assert!(worst_outside(&c, size, s) < 0.01);
        assert_eq!(green_fill(&read_now(LensCorrection::default()), size), 1.);
    }

    /// Only the photo's edge can leave the image. Adobe's warp of a Z5 II
    /// photo (24-70mm f/4 S at 24mm in DX crop, 4000 x 2672) has green's
    /// ratio 1.0149 at the centre and 0.9725 at the corner, and no zoom: the
    /// edge samples just inside.
    #[test]
    fn a_ratio_above_one_inside_the_edges_needs_no_zoom() {
        let k = [1.01488, -0.05341, 0.012095, -0.0010548];
        let knots: Vec<f32> = (0..=32).map(|i| i as f32 / 32.).collect();
        let values = knots
            .iter()
            .map(|r| {
                let r2 = r * r;
                k[0] + k[1] * r2 + k[2] * r2 * r2 + k[3] * r2 * r2 * r2
            })
            .collect();
        let c = read_now(LensCorrection {
            distortion: Radial::new(knots, values),
            ..Default::default()
        });
        assert_eq!(green_fill(&c, [4000., 2672.]), 1.);
    }

    /// Adobe frames by green: red reaching just past the corner (a Coolpix
    /// P1100's 1.0009) does not zoom, and neither does lateral CA alone.
    #[test]
    fn chromatic_aberration_does_not_frame() {
        let ca = [
            Radial::new(vec![0., 1.], vec![1., 1.0009]).unwrap(),
            Radial::new(vec![0., 1.], vec![1., 0.9995]).unwrap(),
        ];
        let alone = read_now(LensCorrection {
            chromatic: Some(ca.clone()),
            ..Default::default()
        });
        assert_eq!(green_fill(&alone, [6000., 4000.]), 1.);
        let barrel = read_now(LensCorrection {
            distortion: Radial::new(vec![0., 1.], vec![1., 0.97]),
            chromatic: Some(ca),
            ..Default::default()
        });
        assert_eq!(green_fill(&barrel, [6000., 4000.]), 1.);
    }

    /// An off-centre optical centre: the corner farthest from it sets the
    /// zoom, and every edge sample stays inside without zooming more than
    /// that needs.
    #[test]
    fn fill_around_an_off_centre_axis_keeps_the_far_corner_inside() {
        let size = [6000f32, 4000.];
        let distortion = Radial::new(
            (0..=12).map(|i| i as f32 / 10.).collect(),
            (0..=12)
                .map(|i| 1. + 0.03 * (i as f32 / 10.).powi(2))
                .collect(),
        );
        let centred = read_now(LensCorrection {
            distortion: distortion.clone(),
            ..Default::default()
        });
        let off = LensCorrection {
            distortion,
            centre: Some([0.46, 0.53]),
            ..Default::default()
        };
        assert!(
            reach([0.46, 0.53], size) > 1.05 && reach(LensCorrection::IMAGE_CENTRE, size) == 1.
        );
        let (a, b) = (green_fill(&centred, size), green_fill(&off, size));
        assert!(b < a, "{a} {b}");
        let worst = worst_outside(&off, size, b);
        assert!((-0.5..0.01).contains(&worst), "{worst}");
        // With the centred fill, the far corner would sample outside.
        assert!(worst_outside(&off, size, a) > 5.);
    }

    /// A correction that folds back has no edge to test; every radius out to
    /// the farthest corner keeps inside instead.
    #[test]
    fn a_folding_correction_keeps_every_radius_inside() {
        let c = read_now(LensCorrection {
            distortion: Radial::new(vec![0., 0.5, 1.], vec![1.2, 1., 0.6]),
            ..Default::default()
        });
        let s = green_fill(&c, [300., 200.]);
        assert!((s - 1. / 1.2).abs() < 1e-4, "{s}");
    }

    #[test]
    fn green_fill_leaves_no_edge_pixel_outside_between_samples() {
        // A table with bumps, the kind Sony's signed values give.
        let c = read_now(LensCorrection {
            distortion: Radial::new(
                (0..11).map(|i| i as f32 / 10.).collect(),
                vec![
                    1.08, 1.06, 1.04, 1.02, 1.02, 1.02, 1.02, 1.02, 1.02, 1.03, 0.99,
                ],
            ),
            ..Default::default()
        });
        let size = [6000f32, 4000f32];
        let worst = worst_outside(&c, size, green_fill(&c, size));
        assert!(worst < 0.5, "{worst}");
    }

    /// A correction stored without a centre reads back as one, and one with
    /// a centre keeps it; a centre outside the image is not valid.
    #[test]
    fn a_correction_stored_before_the_centre_has_none() {
        let c: LensCorrection = serde_json::from_str(
            r#"{"source":"x","vignetting":null,"distortion":null,"chromatic":null}"#,
        )
        .unwrap();
        assert_eq!(c.centre, None);
        assert!(!serde_json::to_string(&c).unwrap().contains("centre"));
        let off = LensCorrection {
            centre: Some([0.4, 0.5]),
            ..c
        };
        let back: LensCorrection =
            serde_json::from_str(&serde_json::to_string(&off).unwrap()).unwrap();
        assert_eq!(back.centre, Some([0.4, 0.5]));
        assert!(
            !LensCorrection {
                centre: Some([1.2, 0.5]),
                ..back
            }
            .validate()
        );
    }

    /// A correction centred off the image centre: the optical centre samples
    /// itself, the map inverts there too, the gain is measured from it, and
    /// the far corner sets the fill.
    #[test]
    fn lens_map_is_centred_on_the_optical_centre() {
        let lens = LensCorrection {
            distortion: Some(Radial {
                knots: vec![0., 0.6, 1.2],
                values: vec![1., 1.02, 1.08],
            }),
            vignetting: Some(Radial {
                knots: vec![0., 1.2],
                values: vec![1., 2.],
            }),
            centre: Some([0.4, 0.55]),
            ..Default::default()
        };
        let map = LensMap::new(&lens, 300., 200., Amounts::FULL);
        // 120, 110 in pixels, offsets from the image centre (150, 100).
        assert!((map.centre[0] + 30.).abs() < 1e-3 && (map.centre[1] - 10.).abs() < 1e-3);
        let [x, y] = map.forward(-30., 10.);
        assert!((x + 30.).abs() < 1e-4 && (y - 10.).abs() < 1e-4, "{x} {y}");
        assert_eq!(map.gain(-30., 10.), 1.);
        // Points the same distance either side of the axis move by the same amount.
        let [a, _] = map.forward(-30. - 60., 10.);
        let [b, _] = map.forward(-30. + 60., 10.);
        assert!(((-30. - a) - (b + 30.)).abs() < 1e-3, "{a} {b}");
        for (x, y) in [(-140., -80.), (140., 90.), (-110., 70.), (-30., 10.)] {
            let [fx, fy] = map.forward(x, y);
            let [bx, by] = map.inverse(fx, fy);
            assert!((bx - x).abs() < 1e-2 && (by - y).abs() < 1e-2, "{x} {y}");
        }
        // The photo's corners sample inside the image, the binding one at its edge.
        let corners = [(-150., -100.), (150., -100.), (-150., 100.), (150., 100.)]
            .map(|(x, y)| map.forward(x, y));
        let margin = |[x, y]: [f32; 2]| (150. - x.abs()).min(100. - y.abs());
        assert!(corners.iter().all(|c| margin(*c) > -0.01), "{corners:?}");
        assert!(corners.iter().any(|c| margin(*c) < 0.05), "{corners:?}");
    }

    /// A correction stored before its optical centre was read keeps its
    /// framing then: Sony's tables the photo's edge in every channel, any
    /// other the whole frame's corners. Read now, every correction keeps
    /// green's samples for the photo's edge inside, and the ZV-1's red,
    /// reaching just past the edge, no longer zooms it.
    #[test]
    fn a_stored_correction_keeps_its_framing_one_read_now_is_framed_by_green() {
        // The ZV-1's distortion: 1.077 at the centre, 0.895 at the corner.
        let read_now = read(&sony_file(&ZV1_VIG, &ZV1_CA, &ZV1_DIST)).unwrap();
        let (w, h) = (5472f32, 3648f32);
        let frame = [0., 0., w, h];
        assert_eq!(read_now.centre, Some(LensCorrection::IMAGE_CENTRE));
        let fill = LensMap::new(&read_now, w, h, Amounts::FULL).fill;
        assert_eq!(fill, read_now.fill_scale(Amounts::FULL, [w, h]));
        assert_eq!(fill, 1.);

        let stored = LensCorrection {
            centre: None,
            ..read_now
        };
        let map = LensMap::new(&stored, w, h, Amounts::FULL);
        let within = stored.fill_scale_within(Amounts::FULL, [w, h], frame);
        assert_eq!(map.fill, within);
        assert!(within < 1. && within > stored.fill_scale_with(Amounts::FULL) + 0.01);
        assert_eq!(map.centre, [0., 0.]);
        // The same tables from anywhere else keep the whole frame's corners inside.
        let other = LensCorrection {
            source: "DNG".into(),
            ..stored.clone()
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
        let bytes = std::fs::read(&path).expect("read");
        let c = read(&bytes).expect("a correction");
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

        // Read now against read as before (the developed image as the whole
        // decoded one, no optical centre): where green samples, in pixels.
        let Some(frame) = developed_frame(&bytes) else {
            return;
        };
        let [w, h] = [frame.crop[2], frame.crop[3]];
        let before = read_with_frame(&bytes, &|| {
            Some(DevelopedFrame {
                decoded: [w, h],
                crop: [0, 0, w, h],
            })
        })
        .map(|c| LensCorrection { centre: None, ..c })
        .expect("a correction");
        let (w, h) = (w as f32, h as f32);
        let (now, then) = (
            LensMap::new(&c, w, h, Amounts::FULL),
            LensMap::new(&before, w, h, Amounts::FULL),
        );
        let mut worst = (0f32, [0f32; 2]);
        for j in 0..=40 {
            for i in 0..=60 {
                let (x, y) = (w * (i as f32 / 60. - 0.5), h * (j as f32 / 40. - 0.5));
                let ([a, b], [p, q]) = (now.forward(x, y), then.forward(x, y));
                let d = (a - p).hypot(b - q);
                if d > worst.0 {
                    worst = (d, [x + w * 0.5, y + h * 0.5]);
                }
            }
        }
        eprintln!(
            "  developed {w} x {h} in {:?}; optical centre {:.2?} px from the image centre",
            frame.decoded, now.centre
        );
        eprintln!(
            "  fill {:.5} now, {:.5} before; green samples up to {:.2} px apart (at {:.0?})",
            now.fill, then.fill, worst.0, worst.1
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
                11_026_883_439_338_406_697,
            ),
            (
                "nikon z 7",
                nikon_file("NIKON Z 7", [8288, 5520], Z7),
                15_728_914_816_814_246_948,
            ),
            (
                "nikon z 8",
                nikon_file("NIKON Z 8", [8280, 5520], Z8),
                11_221_039_355_728_113_879,
            ),
            (
                "coolpix p1100",
                coolpix_file(P1100),
                1_898_652_106_414_007_459,
            ),
        ] {
            assert_eq!(digest(&read(&file)), want, "{name}");
        }
    }
}
