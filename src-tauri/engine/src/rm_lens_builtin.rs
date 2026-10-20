//! A camera's built-in lens correction: the per-shot tables it stores in the
//! RAW for the lens on the camera, at the focal length and aperture used.
//!
//! Fujifilm writes distortion, lateral chromatic aberration and vignetting
//! into every RAF (FujiIFD tags 0xF00B, 0xF00F and 0xF010), and Lightroom
//! applies them by default as the "built-in lens profile". rawler reads the
//! FujiIFD for its own decode and none of these, so without this a Fujifilm
//! photo keeps the darker corners and the colour fringes the camera's JPEG
//! has already corrected.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `v0.4.0` @ a8c5d6d7 (MIT):
//! - `crates/rawmakase-model/src/optics/mod.rs`: `Radial`, `LensCorrection`,
//!   `radial_scale_with` and `fill_scale_with`, as written.
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Fujifilm reader, its
//!   units and the 0.85 vignetting strength, and its test.
//! - `crates/rawmakase-engine/src/develop/image_space.rs`: `LensMap`'s
//!   position map and its Newton inverse.
//! - `docs/lens-corrections.md`: the order (vignetting gain in linear light
//!   at the source pixel, then distortion and CA as one radial remap while
//!   sampling, scaled so corrected corners stay inside the sensor).
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The tables are read with rawler's TIFF reader, as `raf.rs` reads the
//!   FujiIFD, in place of `rawmakase-tiff`, and from the file's bytes rather
//!   than its path.
//! - The amounts are the app's three Lens Correction sliders: distortion and
//!   vignetting as RAWmakase applies Lightroom's (the radial scale moved by
//!   the amount, the gain raised to it), and lateral CA moved the same way as
//!   distortion, where RAWmakase has no amount for it.
//! - `LensMap` works on offsets from the image centre in pixels, as the app's
//!   warp does, rather than on RAWmakase's image positions.
//! - Only what Fujifilm needs. Sony's tables and their framing
//!   (`fill_scale_within`) and a DNG's opcodes come with those formats.

use std::io::Cursor;

use rawler::formats::tiff::IFD;
use serde::{Deserialize, Serialize};

/// FujiIFD, in the TIFF a RAF's header points to.
const FUJI_IFD: u16 = 0xf000;
const FUJI_DISTORTION: u16 = 0xf00b;
const FUJI_CHROMATIC: u16 = 0xf00f;
const FUJI_VIGNETTING: u16 = 0xf010;
/// Where a RAF's header holds the offset of that TIFF, big-endian.
const RAF_TIFF_POINTER: usize = 100;

/// Lightroom renders Fujifilm's vignetting table about 15% weaker in log
/// gain: corners of three X100F photos match Camera Raw at this exponent
/// (+0.08 EV at the corner at full strength). RAWmakase's measurement.
const FUJI_VIGNETTE_STRENGTH: f32 = 0.85;

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
    pub fn is_empty(&self) -> bool {
        self.vignetting.is_none() && self.distortion.is_none() && self.chromatic.is_none()
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
            fill: lens.fill_scale_with(amounts),
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
pub fn read(file: &[u8]) -> Option<LensCorrection> {
    let c = if file.starts_with(b"FUJIFILMCCD-RAW") {
        fuji(file)
    } else {
        None
    }?;
    (!c.is_empty() && c.validate()).then_some(c)
}

/// Fujifilm's tables are signed rationals. The first value is a pixel scale;
/// knots are fractions of the half diagonal, distortion is in percent, CA a
/// radius fraction, vignetting the remaining illumination in percent.
fn fuji(file: &[u8]) -> Option<LensCorrection> {
    let pointer = file.get(RAF_TIFF_POINTER..RAF_TIFF_POINTER + 4)?;
    let base = u32::from_be_bytes(pointer.try_into().ok()?);
    let mut reader = Cursor::new(file);
    let root = IFD::new_root_with_correction(&mut reader, 0, base, 0, 1, &[]).ok()?;
    // Fujifilm writes the FujiIFD pointer as TIFF type 13 (IFD), which rawler
    // keeps as a single byte. The offset is the entry's four bytes.
    let at = base as usize + root.get_entry(FUJI_IFD)?.offset()?;
    file.get(at..at + 4)?;
    let offset = root.endian.read_u32(file, at);
    let fuji = IFD::new(&mut reader, offset, base, 0, root.endian, &[]).ok()?;
    let numbers = |tag: u16| -> Option<Vec<f32>> {
        let v = &fuji.get_entry(tag)?.value;
        (0..v.count())
            .map(|i| v.get_f32(i).ok().flatten())
            .collect()
    };
    // [scale, n knots, n values]
    let pairs = |v: &[f32], f: &dyn Fn(f32) -> f32| {
        let n = v.len().checked_sub(1)? / 2;
        (n >= 2 && v.len() == 2 * n + 1).then(|| {
            Radial::new(
                v[1..=n].to_vec(),
                v[n + 1..].iter().map(|x| f(*x)).collect(),
            )
        })?
    };
    let vignetting = numbers(FUJI_VIGNETTING)
        .and_then(|v| pairs(&v, &|p| (100. / p).powf(FUJI_VIGNETTE_STRENGTH)))
        .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-4));
    let distortion = numbers(FUJI_DISTORTION)
        .and_then(|v| pairs(&v, &|p| 1. + p / 100.))
        .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-6));
    // [scale, n knots, n red, n blue]
    let chromatic = numbers(FUJI_CHROMATIC).and_then(|v| {
        let n = v.len().checked_sub(1)? / 3;
        if n < 2 || v.len() != 3 * n + 1 {
            return None;
        }
        let knots = v[1..=n].to_vec();
        let red = Radial::new(
            knots.clone(),
            v[n + 1..=2 * n].iter().map(|x| 1. + x).collect(),
        )?;
        let blue = Radial::new(knots, v[2 * n + 1..].iter().map(|x| 1. + x).collect())?;
        [&red, &blue]
            .iter()
            .any(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-7))
            .then_some([red, blue])
    });
    Some(LensCorrection {
        source: "Fujifilm built-in".into(),
        default_on: true,
        vignetting,
        distortion,
        chromatic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (tag, type, count, data)
    type Entry = (u16, u16, u32, Vec<u8>);

    /// Little-endian TIFF with the given directories, each pointed to by the
    /// previous one's first entry. RAWmakase's builder, as written.
    fn tiff(dirs: &[Vec<Entry>]) -> Vec<u8> {
        let mut out = b"II*\0\x08\0\0\0".to_vec();
        for (d, entries) in dirs.iter().enumerate() {
            let start = out.len();
            let data_start = start + 2 + entries.len() * 12 + 4;
            let mut data: Vec<u8> = Vec::new();
            out.extend((entries.len() as u16).to_le_bytes());
            for (tag, kind, count, bytes) in entries {
                out.extend(tag.to_le_bytes());
                out.extend(kind.to_le_bytes());
                out.extend(count.to_le_bytes());
                if bytes.len() <= 4 {
                    let mut v = bytes.clone();
                    v.resize(4, 0);
                    out.extend(v);
                } else {
                    out.extend(((data_start + data.len()) as u32).to_le_bytes());
                    data.extend(bytes);
                }
            }
            out.extend(0u32.to_le_bytes());
            out.extend(data);
            // A pointer entry value of 0 is patched to the next directory.
            if d + 1 < dirs.len() {
                let next = out.len() as u32;
                let value = start + 2 + 8;
                out[value..value + 4].copy_from_slice(&next.to_le_bytes());
            }
        }
        out
    }

    fn rationals(v: &[f64]) -> Vec<u8> {
        v.iter()
            .flat_map(|x| {
                let n = (*x * 1_000_000.).round() as i32;
                [n.to_le_bytes(), 1_000_000i32.to_le_bytes()].concat()
            })
            .collect()
    }

    /// A RAF whose FujiIFD holds `entries`, its pointer of type 13 (IFD) as
    /// Fujifilm writes it.
    fn raf(entries: Vec<Entry>) -> Vec<u8> {
        let fuji = tiff(&[vec![(FUJI_IFD, 13, 1, vec![0; 4])], entries]);
        let mut raf = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
        raf.resize(128, 0);
        raf[RAF_TIFF_POINTER..RAF_TIFF_POINTER + 4].copy_from_slice(&128u32.to_be_bytes());
        raf.extend(fuji);
        raf
    }

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
    fn reads_fujifilm_raf_tables() {
        let knots: Vec<f64> = (0..=10).map(|i| i as f64 / 10.).collect();
        let mut vig = vec![327.7];
        vig.extend(&knots);
        vig.extend([
            100., 99.5, 98.8, 97.5, 95.4, 92.2, 87.7, 82.4, 75.8, 68.3, 60.4,
        ]);
        let mut dist = vec![327.7];
        dist.extend(&knots);
        dist.extend(std::iter::repeat_n(0., 11));
        let mut ca = vec![327.7];
        ca.extend(&knots);
        ca.extend((0..=10).map(|i| i as f64 * 1e-4));
        ca.extend(std::iter::repeat_n(0., 11));
        let file = raf(vec![
            (FUJI_DISTORTION, 10, 23, rationals(&dist)),
            (FUJI_CHROMATIC, 10, 34, rationals(&ca)),
            (FUJI_VIGNETTING, 10, 23, rationals(&vig)),
        ]);
        let c = read(&file).unwrap();
        assert!(c.default_on);
        assert!(c.distortion.is_none(), "identity distortion is omitted");
        let v = c.vignetting.as_ref().unwrap();
        let gain = |p: f32| (100. / p).powf(FUJI_VIGNETTE_STRENGTH);
        assert!((v.eval(1.) - gain(60.4)).abs() < 1e-4);
        assert!((v.eval(0.85) - (gain(75.8) + gain(68.3)) / 2.).abs() < 1e-4);
        let [red, blue] = c.chromatic.as_ref().unwrap();
        assert!((red.eval(1.) - 1.001).abs() < 1e-6);
        assert_eq!(blue.eval(1.), 1.);
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

    /// A real RAF (AG_RAW): its tables at a few radii, and the zoom that
    /// keeps the corners inside. Prints; asserts only that one was read.
    #[test]
    #[ignore = "reads a local RAF named by AG_RAW; run by hand"]
    fn a_real_rafs_tables() {
        let path = std::env::var("AG_RAW").expect("AG_RAW");
        let c = read(&std::fs::read(&path).expect("read")).expect("a correction");
        let at = |r: &Option<Radial>| -> Vec<f32> {
            r.as_ref()
                .map_or(vec![], |r| [0.5, 0.8, 1.].map(|x| r.eval(x)).to_vec())
        };
        eprintln!();
        eprintln!("{path}: {}", c.source);
        eprintln!(
            "  vignetting gain at r 0.5/0.8/1: {:.3?}",
            at(&c.vignetting)
        );
        eprintln!("  distortion at r 0.5/0.8/1: {:.4?}", at(&c.distortion));
        if let Some([red, blue]) = &c.chromatic {
            eprintln!(
                "  CA red/blue at r 1: {:.5} {:.5}",
                red.eval(1.),
                blue.eval(1.)
            );
        }
        eprintln!("  fill {:.4}", c.fill_scale_with(Amounts::FULL));
    }
}
