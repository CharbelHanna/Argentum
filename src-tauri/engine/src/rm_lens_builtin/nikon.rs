//! Nikon's built-in lens correction, read from a NEF or a Coolpix NRW.
//!
//! Nikon Z bodies store the camera's distortion, vignetting and lateral CA
//! model in the raw SubIFD's NEFInfo (tag 0xC7D5, ExifTool's NikonNEFInfo):
//! a TIFF after a "Nikon\0" header and four version bytes, whose entries 5
//! to 8 are records led by "0100": distortion, vignetting, lateral CA and
//! the optical centre. Values are signed rationals, polynomials of the
//! radius r with coefficients for descending powers down to r^1
//! (`descending`), r being 1 at the default crop's corner farthest from the
//! optical centre. Distortion, after a mode byte and a count of 4 at 0x10:
//! green's source radius over r is (1 + k(r)) / (1 + x), x at 0x40.
//! Vignetting, a count of 8 at 0x10: gain 1 + a v(r), a at 0x60. CA, for red
//! at 0x10 and blue at 0x34: a count of 3, the coefficients and a constant
//! e, the channel's scale over green's at r being 1 + e + c(r). The centre:
//! x and y in pixels from the image's.
//!
//! Adobe DNG Converter applies it as opcodes for a lens that needs the
//! correction, which Lightroom always applies: on, the camera correcting
//! every part it stores. For a lens whose correction is optional it embeds
//! it as the "Camera Settings" profile, which follows Enable Profile
//! Corrections: off, and correcting all of it once switched on. Nothing at
//! all when the camera's Auto Distortion Control was off. RAWmakase measured
//! it against Adobe's opcodes and profiles on 101 NEFs, to within 0.4 px.
//!
//! A Coolpix P950, P1000 or P1100 NRW's NEFInfo holds distortion and
//! lateral CA tables in records 2 and 3 instead (`coolpix_tables`), which
//! Adobe writes as opcodes: on, the camera correcting both. RAWmakase
//! matched Adobe's warp within 0.55 to 1.2 px on the three bodies' samples.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `ae29938` (MIT), `crates/rawmakase-model/src/lens/embedded.rs`:
//! - the Nikon reader (`nikon`, `descending`, and `make`, IFD0's Make), its
//!   units, and its tests.
//!
//! RAWmakase `d37942f` (MIT), the same file:
//! - the Coolpix reader (`coolpix_tables`, `COOLPIX_CA_UNIT`), the records
//!   it is read from, and its test.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The NEFInfo and its records are read with rawler's TIFF reader in place
//!   of `rawmakase-tiff`, and from the file's bytes rather than its path.
//! - RAWmakase's `default_on` (mode bit 1, and always for the Coolpix
//!   tables) becomes this correction's `default_on` and `always`
//!   (`LensCorrection::always_correcting`): every part it has, for a lens
//!   that needs it and for the Coolpix; none for an optional one.
//! - Radius 1 is the developed image's half diagonal, as in the other
//!   readers. The developed image is rawler's raw image at its default crop,
//!   so it stands for both RAWmakase's decoded image and its default crop:
//!   rawler's crops of the Z 7 and Z 8 (8256 x 5504) and of the P950, P1000
//!   and P1100 (8 and 7 pixels off each side) are the default crops
//!   RAWmakase's tests use. The Z reader
//!   reads the size with rawler (`developed_size_px`), which places the
//!   optical centre's offset; the Coolpix tables are spread to the default
//!   crop's corner, radius 1 here, and need no size.
//! - As in RAWmakase at these commits, the correction is centred on the
//!   image; the optical centre only sets where a Z body's radius 1 is.

use std::io::Cursor;

use rawler::bits::Endian;
use rawler::formats::tiff::{Entry, IFD, Value};
use rawler::tags::TiffCommonTag;

use super::fit::{knots, spline};
use super::{LensCorrection, Parts, Radial, SUB_IFDS};

/// NEFInfo, in the raw image's SubIFD.
pub(super) const NEF_INFO: u16 = 0xc7d5;

/// Red's and blue's displacement from green in a Coolpix CA table, as a
/// fraction of the half diagonal: measured by RAWmakase against Adobe's
/// warps of the P950, P1000 and P1100, where it is the same to within 1%.
// RAWmakase d37942f: crates/rawmakase-model/src/lens/embedded.rs
const COOLPIX_CA_UNIT: f64 = 1. / 3792.;

/// The value of a polynomial whose coefficients are for descending powers
/// of `q`, the last for q^1, as Nikon stores them.
// RAWmakase ae29938: crates/rawmakase-model/src/lens/embedded.rs
fn descending(c: &[f64], q: f64) -> f64 {
    c.iter().fold(0., |sum, k| (sum + k) * q)
}

/// An entry's bytes as stored, whatever rawler made of its type.
fn stored(e: &Entry) -> Option<&[u8]> {
    match &e.value {
        Value::Undefined(b) | Value::Byte(b) => Some(b),
        _ => None,
    }
}

/// `None` for a file that is not a Nikon TIFF, has no NEFInfo, or one whose
/// records do not parse or whose correction the camera had off.
pub(super) fn read(file: &[u8]) -> Option<LensCorrection> {
    if !super::is_tiff(file) {
        return None;
    }
    let root =
        IFD::new_root_with_correction(&mut Cursor::new(file), 0, 0, 0, 1, &[SUB_IFDS]).ok()?;
    // RAWmakase ae29938: crates/rawmakase-model/src/lens/embedded.rs
    let make = (root.get_entry(TiffCommonTag::Make))
        .and_then(|e| e.value.as_string())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !make.starts_with("nikon") {
        return None;
    }
    // In the raw image's SubIFD.
    let info = (root.get_sub_ifd_all(SUB_IFDS)?.iter())
        .find_map(|sub| stored(sub.get_entry(NEF_INFO)?))?;
    if !info.starts_with(b"Nikon\0") {
        return None;
    }
    let n = IFD::new_root_with_correction(&mut Cursor::new(info), 0, 10, 0, 1, &[]).ok()?;
    let record = |tag: u16, len| {
        (n.get_entry(tag).and_then(stored)).filter(|b| b.len() >= len && b.starts_with(b"0100"))
    };
    let (d, v, ca, centre) = (
        record(5, 0x48),
        record(6, 0x68),
        record(7, 0x58),
        record(8, 0x14),
    );
    // RAWmakase d37942f: crates/rawmakase-model/src/lens/embedded.rs
    // Coolpix NRW files hold tables in records 2 and 3 instead.
    let coolpix = (record(2, 8 + 8 * 17), record(3, 8 + 8 * 34));
    let little = n.endian == Endian::Little;
    let int = |b: &[u8], at: usize| {
        let c = b.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(c)
        } else {
            u32::from_be_bytes(c)
        })
    };
    let rational = |b: &[u8], at: usize| {
        let (num, den) = (int(b, at)? as i32, int(b, at + 4)? as i32);
        (den != 0).then(|| f64::from(num) / f64::from(den))
    };
    // A count of `n` rationals at `at`, followed by them.
    let coefficients = |b: &[u8], at: usize, n: u32| -> Option<Vec<f64>> {
        if int(b, at)? != n {
            return None;
        }
        (0..n as usize)
            .map(|i| rational(b, at + 4 + 8 * i))
            .collect()
    };
    // RAWmakase d37942f: crates/rawmakase-model/src/lens/embedded.rs
    let Some(d) = d else {
        let (Some(distortion), Some(ca)) = coolpix else {
            return None;
        };
        return coolpix_tables(distortion, ca, &rational);
    };
    // Bit 0 of the mode is the camera's Auto Distortion Control, bit 1 a lens
    // that needs the correction. Adobe applies nothing, not even vignetting,
    // without the first (D6, D780).
    let mode = d[4];
    if mode & 1 == 0 {
        return None;
    }
    let k = coefficients(d, 0x10, 4)?;
    let scale = 1. / (1. + rational(d, 0x40)?);
    // The radius is 1 at the default crop's corner farthest from the optical
    // centre, which tag 8 places in pixels from the image centre. The
    // correction is centred on the image.
    let [dx, dy] = [4, 12].map(|at| centre.and_then(|c| rational(c, at)).unwrap_or(0.));
    let (w, h) = super::developed_size_px(file)?;
    let (w, h) = (w as f64, h as f64);
    let unit = (w * 0.5 + dx.abs()).hypot(h * 0.5 + dy.abs());
    let half = w.hypot(h) * 0.5;
    if !(half > 0. && unit > 0. && scale.is_finite() && scale > 0.) {
        return None;
    }
    let curve = |f: &dyn Fn(f64) -> f64| {
        let values = knots()
            .into_iter()
            .map(|r| f(f64::from(r) * half / unit) as f32);
        Radial::new(knots(), values.collect())
    };
    let changes = |r: &Radial, by: f32| r.values.iter().any(|v| (v - 1.).abs() > by);
    let distortion = curve(&|q| scale * (1. + descending(&k, q))).filter(|r| changes(r, 1e-6));
    let vignetting = v
        .and_then(|v| {
            let c = coefficients(v, 0x10, 8)?;
            let strength = rational(v, 0x60)?;
            curve(&|q| 1. + strength * descending(&c, q))
        })
        .filter(|r| changes(r, 1e-4));
    // Red's and blue's scale at the output radius: a constant after the
    // coefficients.
    let chromatic = ca
        .and_then(|b| {
            let plane = |at: usize| {
                let c = coefficients(b, at, 3)?;
                let constant = rational(b, at + 28)?;
                curve(&|q| 1. + constant + descending(&c, q))
            };
            Some([plane(0x10)?, plane(0x34)?])
        })
        .filter(|[a, b]| changes(a, 1e-7) || changes(b, 1e-7));
    let lens = LensCorrection {
        source: "Nikon built-in".into(),
        default_on: false,
        vignetting,
        distortion,
        chromatic,
        always: None,
    };
    // Adobe writes a lens that needs it as its DNGs' opcodes, which always
    // apply; an optional one as a profile.
    let always = if mode & 2 != 0 {
        Parts::ALL
    } else {
        Parts::default()
    };
    Some(lens.always_correcting(always))
}

/// A Coolpix NRW's tables (P950, P1000, P1100): record 2, after "0100" and
/// the header bytes 3 3 17 1, holds 17 rationals, the distortion in percent
/// at radii spread evenly from the centre to the default crop's corner
/// (green's source radius over the output radius is 1 + d/100, through a
/// natural spline); record 3, after 3 34 1 0, red's and then blue's
/// displacement from green at the same radii, 17 each. The A1000's record 2
/// has another header and is not read.
// RAWmakase d37942f: crates/rawmakase-model/src/lens/embedded.rs
fn coolpix_tables(
    distortion: &[u8],
    ca: &[u8],
    rational: &dyn Fn(&[u8], usize) -> Option<f64>,
) -> Option<LensCorrection> {
    let table = |b: &[u8], header: [u8; 4], n: usize| -> Option<Vec<f64>> {
        if b.get(4..8)? != header {
            return None;
        }
        (0..n).map(|i| rational(b, 8 + 8 * i)).collect()
    };
    let d: Vec<f64> = table(distortion, [3, 3, 17, 1], 17)?
        .iter()
        .map(|d| 1. + d / 100.)
        .collect();
    let ca = table(ca, [3, 34, 1, 0], 34);
    // The tables' radius 1, the default crop's corner, is the developed
    // image's.
    let curve = |f: &dyn Fn(f64) -> f64| {
        let values = knots().into_iter().map(|r| f(f64::from(r)) as f32);
        Radial::new(knots(), values.collect())
    };
    let ratio = spline(&d);
    // Scaled, as Adobe's warp is, so the widest ratio out to the corner is 1.
    let widest = (0..=64)
        .map(|i| ratio(f64::from(i) / 64.))
        .fold(1., f64::max);
    let changes = |r: &Radial, by: f32| r.values.iter().any(|v| (v - 1.).abs() > by);
    let distortion = curve(&|q| ratio(q) / widest).filter(|r| changes(r, 1e-6));
    // A displacement at green's source radius, as a scale of it at the output
    // radius.
    let chromatic = ca
        .and_then(|ca| {
            let plane = |t: &[f64]| {
                let shift = spline(t);
                curve(&|q| {
                    let q = q.max(1. / 64.);
                    1. + shift(q) * COOLPIX_CA_UNIT / (q * ratio(q))
                })
            };
            Some([plane(&ca[..17])?, plane(&ca[17..])?])
        })
        .filter(|[a, b]| changes(a, 1e-7) || changes(b, 1e-7));
    let lens = LensCorrection {
        source: "Nikon built-in".into(),
        default_on: false,
        vignetting: None,
        distortion,
        chromatic,
        always: None,
    };
    // Adobe writes them as its DNGs' opcodes, which always apply.
    Some(lens.always_correcting(Parts::ALL))
}

#[cfg(test)]
mod tests {
    use crate::rm_lens_builtin::test_files::{P1100, Z7, Z8, coolpix_file, nikon_file};
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::{Amounts, LensCorrection, Parts, developed_size_px, read};

    /// Source radius in pixels at output radius `px` from Adobe's
    /// WarpRectilinear coefficients, whose radius 1 is `unit` pixels.
    fn adobe(k: [f64; 4], unit: f64, px: f64) -> f64 {
        let r2 = (px / unit).powi(2);
        px * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * k[3])))
    }

    /// Largest difference in pixels, out to the corner of the developed
    /// image of `size`, between the source radii of `c` and Adobe's warp.
    fn largest_difference(
        c: &LensCorrection,
        size: [f64; 2],
        adobe: &dyn Fn(usize, f64) -> f64,
    ) -> f64 {
        let half = size[0].hypot(size[1]) / 2.;
        (0..=200)
            .flat_map(|i| {
                let px = half * f64::from(i) / 200.;
                let scale = c.radial_scale_with((px / half) as f32, Amounts::FULL);
                (0..3).map(move |ch| (px * f64::from(scale[ch]) - adobe(ch, px)).abs())
            })
            .fold(0., f64::max)
    }

    /// Against the opcodes Adobe DNG Converter 18.7 writes for the Z 7 file:
    /// its WarpRectilinear and FixVignetteRadial, radius 1 at the 8288 x 5520
    /// image's corner farthest from their centre (4143.2, 2762.0). rawler
    /// develops its 8256 x 5504 default crop, centred on the same point.
    #[test]
    fn reads_nikon_corrections_as_adobe_does() {
        let file = nikon_file("NIKON Z 7", [8288, 5520], Z7);
        assert_eq!(developed_size_px(&file), Some((8256, 5504)));
        let c = read(&file).unwrap();
        assert!(c.default_on);
        assert_eq!(c.always, Some(Parts::ALL));
        assert_eq!(c.source, "Nikon built-in");
        let warp = [
            [
                0.9417050022703349,
                0.0341640171586027,
                0.007692219047459261,
                0.0017741287665110545,
            ],
            [
                0.9417976348406594,
                0.03441966339031103,
                0.0077043443801977285,
                0.0017867041750981045,
            ],
            [
                0.9419519123251883,
                0.03469149168629527,
                0.0076797231784166645,
                0.0018015685664133994,
            ],
        ];
        let unit = 4144.8f64.hypot(2762.);
        let off = largest_difference(&c, [8256., 5504.], &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.25, "{off} px from Adobe's warp");
        // Without the correction, halfway to the corner is 120 px out.
        assert!(adobe(warp[1], unit, 2481.) < 2481. - 110.);
        let k = [
            0.2572594881057739,
            -0.4153914023190737,
            1.1038962956517935,
            -0.5901794414967299,
        ];
        let half = 8256f64.hypot(5504.) / 2.;
        for px in [1000., 2500., 4000., 4962.] {
            let r2 = (px / unit).powi(2);
            let gain = 1. + r2 * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * k[3])));
            let ours = f64::from(c.vignetting_gain((px / half) as f32, 1.));
            assert!((ours - gain).abs() < 0.003, "{px}: {ours} against {gain}");
        }
    }

    /// The Z 8's lens has an optional correction, which Adobe embeds in its
    /// DNG as the "Camera Settings" lens profile: its distortion (radius in
    /// focal lengths, 24mm on a sensor of crop factor 1.003485), piecewise
    /// vignetting and the CA of its warp.
    #[test]
    fn reads_nikon_optional_correction_as_adobe_does() {
        let c = read(&nikon_file("NIKON Z 8", [8280, 5520], Z8)).unwrap();
        // Enable Profile Corrections uses it when no Adobe profile matches:
        // off, and all of it once switched on.
        assert!(!c.default_on);
        assert_eq!(c.always, None);
        let profile = |px: f64| {
            let rho = px / 8256f64.hypot(5504.) * 43.2666 / 1.003485 / 24.;
            let r2 = rho * rho;
            px * 0.9808 * (1. + r2 * (0.021103 + r2 * (-0.000082 + r2 * 0.00394)))
        };
        // The warp's red and blue planes (green's is the identity), radius 1
        // at the corner farthest from (4133.1, 2764.6).
        let plane = |k: [f64; 4], px: f64| adobe(k, 4146.9f64.hypot(2764.6), px) / px;
        let red = [
            0.9998067206878185,
            0.0007524371460466559,
            -0.0005651242131282319,
            0.00013040478604332164,
        ];
        let blue = [
            1.000369837685565,
            -0.0007921865303304411,
            0.0006318055362242725,
            -0.0001457917814567964,
        ];
        let off = largest_difference(&c, [8256., 5504.], &|ch, px| match ch {
            0 => profile(px) * plane(red, px),
            2 => profile(px) * plane(blue, px),
            _ => profile(px),
        });
        assert!(off < 0.45, "{off} px from Adobe's profile");
        // Piecewise vignetting: radius in focal lengths, the share of light kept.
        let half = 8256f64.hypot(5504.) / 2.;
        for (rho, kept) in [
            (0.290603, 0.979004),
            (0.581206, 0.918796),
            (0.900869, 0.807075),
        ] {
            let px = rho * 24. * 1.003485 / 43.2666 * 8256f64.hypot(5504.);
            let ours = f64::from(c.vignetting_gain((px / half) as f32, 1.));
            assert!(
                (1. / ours - kept).abs() < 0.001,
                "{rho}: {} against {kept}",
                1. / ours
            );
        }
    }

    /// Against the warp Adobe DNG Converter 18.7 writes for the P1100 file,
    /// radius 1 at the 4624 x 3470 image's corner. rawler develops its
    /// 4608 x 3456 default crop, centred on the same point.
    #[test]
    fn reads_coolpix_tables_as_adobe_does() {
        let c = read(&coolpix_file(P1100)).unwrap();
        assert!(c.default_on && c.vignetting.is_none());
        // The camera corrects both parts it stores; lensfun the shading.
        assert_eq!(
            c.always,
            Some(Parts {
                vignetting: false,
                ..Parts::ALL
            })
        );
        // Only red's CA, past the corner as in Adobe's warp, zooms in at all.
        assert!(c.fill_scale_with(Amounts::FULL) > 0.999);
        let warp = [
            [
                0.9770807182516118,
                0.02199974273005716,
                0.0017052145920701364,
                9.575715160584196e-05,
            ],
            [
                0.9763794594830819,
                0.02202587112403992,
                0.0016701011495196144,
                0.00010085923996837375,
            ],
            [
                0.9767193908434475,
                0.022103375850249615,
                0.0016335961178098911,
                0.00010094050312599621,
            ],
        ];
        let unit = 2312f64.hypot(1735.);
        let off = largest_difference(&c, [4608., 3456.], &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.75, "{off} px from Adobe's warp");
        // The A1000's layout (header 3 2 17 1) is not read.
        let a1000 = P1100[0].replacen("03 03 11 01", "03 02 11 01", 1);
        assert!(read(&coolpix_file([&a1000, P1100[1]])).is_none());
    }

    #[test]
    fn nikon_correction_off_in_camera_is_not_read() {
        // Mode 2: a lens that needs it, with Auto Distortion Control off (D780).
        let off = Z7[0].replacen("03 01 00 00", "02 01 00 00", 1);
        let records = [&off[..], Z7[1], Z7[2], Z7[3]];
        assert!(read(&nikon_file("NIKON Z 7", [8288, 5520], records)).is_none());
    }
}
