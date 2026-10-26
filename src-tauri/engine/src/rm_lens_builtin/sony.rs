//! Sony's built-in lens correction, read from an ARW.
//!
//! Sony writes distortion, lateral chromatic aberration and vignetting into
//! an ARW's raw SubIFD (0x7032 vignetting, 0x7035 CA, 0x7037 distortion).
//! Adobe DNG Converter writes the lateral CA of every Sony body with tables,
//! and a fixed-lens compact's distortion, as opcodes, which Lightroom always
//! applies; never the vignetting. So the correction is on, the camera always
//! corrects those parts, and its other tables stand in for a lens profile.
//! Sony's tables carry the camera's own framing (a ZV-1's distortion is 1.077
//! at the centre), so they are filled only as far as the photo's edge needs,
//! which keeps the camera's crop (`LensMap`'s fill, in `rm_lens_builtin.rs`).
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `v0.4.0` @ a8c5d6d7 (MIT):
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Sony reader, its
//!   units, and its tests.
//!
//! RAWmakase `44b21d5` (MIT), the same file:
//! - which parts Lightroom applies (`sony_applies_distortion`), the
//!   distortion sampled through a natural cubic spline (`fit::spline`), and
//!   the tests that changed with them.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The tables are read with rawler's TIFF reader in place of
//!   `rawmakase-tiff`, and from the file's bytes rather than its path.
//! - RAWmakase reads two corrections: what Lightroom applies, and all three
//!   tables standing in for a profile. Here they are one, all three tables,
//!   with `always` naming the parts Lightroom applies
//!   (`LensCorrection::always_correcting`).
//! - Not copied from 44b21d5: its rule that a profile's amounts leave an
//!   always-applied correction in full (`Recipe::lens_amounts`); the app's
//!   Lens Correction amounts scale every source.

use std::io::Cursor;

use rawler::formats::tiff::IFD;
use rawler::tags::TiffCommonTag;

use super::fit::{knots, spline};
use super::{LensCorrection, Parts, Radial, SUB_IFDS};

pub(super) const SONY_VIGNETTING: u16 = 0x7032;
pub(super) const SONY_CHROMATIC: u16 = 0x7035;
pub(super) const SONY_DISTORTION: u16 = 0x7037;

/// Sony models whose stored distortion Lightroom applies: the fixed-lens
/// compacts (RX100, RX10, RX0, HX, ZV-1), not the RX1 series, which Adobe
/// corrects with its own profile only, nor bodies with interchangeable lenses.
// RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
fn sony_applies_distortion(model: &str) -> bool {
    let m = (model.trim_matches(|c: char| c == '\0' || c.is_whitespace())).to_ascii_uppercase();
    (m.starts_with("DSC-") && m != "DSC-RX1" && !m.starts_with("DSC-RX1R")) || m.starts_with("ZV-1")
}

/// Sony's tables are signed shorts led by the number of values and padded
/// with zeros to a fixed length (ZV-1: 11 distortion knots in 16 slots).
/// Knots are spread evenly from the centre to the corner. Distortion is in
/// 2^-14, CA in 2^-21 units; vignetting v restores 2^(2^(v/8192 - 1) - 0.5).
/// `None` for a file that is not a TIFF or has no SubIFD.
pub(super) fn read(file: &[u8]) -> Option<LensCorrection> {
    if !super::is_tiff(file) {
        return None;
    }
    let root =
        IFD::new_root_with_correction(&mut Cursor::new(file), 0, 0, 0, 1, &[SUB_IFDS]).ok()?;
    // RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
    let model = (root.get_entry(TiffCommonTag::Model))
        .and_then(|e| e.value.as_string().cloned())
        .unwrap_or_default();
    // An ARW's raw image directory is the first.
    let sub = root.get_sub_ifd_all(SUB_IFDS)?.first()?;
    let numbers = |tag: u16| -> Option<Vec<f32>> {
        let v = &sub.get_entry(tag)?.value;
        (0..v.count())
            .map(|i| v.get_f32(i).ok().flatten())
            .collect()
    };
    let knots_of = |n: usize| {
        (0..n)
            .map(|i| i as f32 / (n - 1) as f32)
            .collect::<Vec<_>>()
    };
    // The count's values, from storage that may be longer: a ZV-1 stores 11
    // distortion knots in 16 slots. Anything stored past the count must be
    // zero padding.
    let counted = |v: Option<Vec<f32>>, per_count: usize| {
        let v = v?;
        let count = *v.first()? as usize;
        let values = v.get(1..=count)?;
        (count >= 2 * per_count
            && count.is_multiple_of(per_count)
            && v[count + 1..].iter().all(|x| *x == 0.))
        .then(|| values.to_vec())
    };
    let single = |v: Option<Vec<f32>>, f: &dyn Fn(f32) -> f32| {
        let v = counted(v, 1)?;
        Radial::new(knots_of(v.len()), v.iter().map(|x| f(*x)).collect())
    };
    let vignetting = single(numbers(SONY_VIGNETTING), &|v| {
        (2f32.powf(v / 8192. - 1.) - 0.5).exp2()
    })
    .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-4));
    // RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
    // A natural cubic spline through the knots, as the camera's correction is
    // smooth: against Adobe's warp it halves the difference on the RX100 III
    // to V and ZV-1.
    let distortion = counted(numbers(SONY_DISTORTION), 1)
        .and_then(|v| {
            let v: Vec<f64> = v.iter().map(|d| 1. + f64::from(*d) / 16384.).collect();
            let at = spline(&v);
            Radial::new(
                knots(1.),
                knots(1.)
                    .into_iter()
                    .map(|r| at(f64::from(r)) as f32)
                    .collect(),
            )
        })
        .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-6));
    let chromatic = counted(numbers(SONY_CHROMATIC), 2).and_then(|v| {
        let n = v.len() / 2;
        let scale = |s: &[f32]| s.iter().map(|x| 1. + x / 2_097_152.).collect();
        Some([
            Radial::new(knots_of(n), scale(&v[..n]))?,
            Radial::new(knots_of(n), scale(&v[n..]))?,
        ])
    });
    let tables = LensCorrection {
        source: LensCorrection::SONY_BUILTIN.into(),
        default_on: false,
        vignetting,
        distortion,
        chromatic,
        always: None,
        centre: None,
    };
    // RAWmakase 44b21d5: crates/rawmakase-model/src/lens/embedded.rs
    // Adobe DNG Converter writes the lateral CA of every Sony body with
    // tables, and a compact's distortion, as opcodes, which Lightroom always
    // applies; never the vignetting.
    Some(tables.always_correcting(Parts {
        distortion: sony_applies_distortion(&model),
        chromatic: true,
        vignetting: false,
    }))
}

#[cfg(test)]
mod tests {
    use crate::rm_lens_builtin::test_files::{
        ZV1_CA, ZV1_DIST, ZV1_VIG, sony_file, sony_model_file,
    };
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::{LensCorrection, Parts, read};

    /// What Lightroom applies on a body with interchangeable lenses.
    const CA_ONLY: Parts = Parts {
        distortion: false,
        chromatic: true,
        vignetting: false,
    };

    #[test]
    fn reads_sony_subifd_tables() {
        let mut vig = vec![16i16];
        vig.extend((0..16).map(|i| i * 800));
        let mut dist = vec![16i16];
        dist.extend((0..16).map(|i| i * 10));
        let mut ca = vec![32i16];
        ca.extend(std::iter::repeat_n(-384, 16));
        ca.extend(std::iter::repeat_n(0, 16));
        // A body that names no model.
        let c = read(&sony_model_file("", &vig, &ca, &dist)).unwrap();
        // Lightroom applies the lateral CA, not this body's distortion or vignetting.
        assert!(c.default_on);
        assert_eq!(c.always, Some(CA_ONLY));
        assert_eq!(c.source, LensCorrection::SONY_BUILTIN);
        let v = c.vignetting.as_ref().unwrap();
        assert!((v.eval(0.) - 1.).abs() < 1e-6);
        let corner = 2f32.powf(2f32.powf(12000. / 8192. - 1.) - 0.5);
        assert!((v.eval(1.) - corner).abs() < 1e-5);
        assert!((c.distortion.as_ref().unwrap().eval(1.) - (1. + 150. / 16384.)).abs() < 1e-6);
        let [red, blue] = c.chromatic.as_ref().unwrap();
        assert!((red.eval(0.5) - (1. - 384. / 2_097_152.)).abs() < 1e-7);
        assert_eq!(blue.eval(0.5), 1.);
    }

    #[test]
    fn reads_sony_tables_padded_to_fixed_slots() {
        let c = read(&sony_file(&ZV1_VIG, &ZV1_CA, &ZV1_DIST)).unwrap();
        let dist = c.distortion.as_ref().unwrap();
        // The 11 knots, through which a spline is sampled at 33.
        for (i, v) in [1265., 1218., 1083., 860., 562., 209.]
            .into_iter()
            .enumerate()
        {
            let at = dist.eval(i as f32 / 10.);
            assert!((at - (1. + v / 16384.)).abs() < 1e-4, "{i}: {at}");
        }
        assert!((dist.eval(1.) - (1. - 1725. / 16384.)).abs() < 1e-6);
        // A compact: Lightroom applies its distortion too, not its vignetting.
        assert_eq!(
            c.always,
            Some(Parts {
                distortion: true,
                ..CA_ONLY
            })
        );
        let [red, blue] = c.chromatic.as_ref().unwrap();
        assert_eq!(red.knots.len(), 11);
        assert!((red.eval(1.) - (1. + 1408. / 2_097_152.)).abs() < 1e-7);
        assert!((blue.eval(0.1) - (1. + 1280. / 2_097_152.)).abs() < 1e-7);
        assert!((blue.eval(1.) - (1. + 256. / 2_097_152.)).abs() < 1e-7);
        assert_eq!(c.vignetting.as_ref().unwrap().knots.len(), 16);
    }

    /// Adobe writes the stored distortion of Sony's fixed-lens compacts as an
    /// opcode, but neither an interchangeable-lens body's nor the RX1 series'.
    #[test]
    fn sony_distortion_applies_on_compacts_only() {
        for (model, applies) in [
            ("DSC-RX100M7", true),
            ("DSC-RX10M4", true),
            ("DSC-HX99", true),
            ("ZV-1M2", true),
            ("DSC-RX1RM3", false),
            ("DSC-RX1", false),
            ("ILCE-7M4", false),
            ("ZV-E10", false),
        ] {
            let c = read(&sony_model_file(model, &ZV1_VIG, &ZV1_CA, &ZV1_DIST)).unwrap();
            let always = c.always.unwrap();
            assert_eq!(always.distortion, applies, "{model}");
            assert!(always.chromatic && !always.vignetting, "{model}");
            assert!(c.distortion.is_some() && c.vignetting.is_some(), "{model}");
        }
    }

    #[test]
    fn rejects_malformed_sony_tables() {
        fn with<const N: usize>(mut v: [i16; N], i: usize, x: i16) -> [i16; N] {
            v[i] = x;
            v
        }
        for (name, dist, ca) in [
            (
                "data past the count",
                with(ZV1_DIST, 16, 1),
                with(ZV1_CA, 32, 1),
            ),
            (
                "count past the stored values",
                with(ZV1_DIST, 0, 17),
                with(ZV1_CA, 0, 34),
            ),
            ("a single knot", with(ZV1_DIST, 0, 1), with(ZV1_CA, 0, 2)),
        ] {
            let c = read(&sony_file(&ZV1_VIG, &ca, &dist)).unwrap();
            assert!(c.distortion.is_none() && c.chromatic.is_none(), "{name}");
            // Lightroom has nothing to apply: the vignetting only stands in
            // for a profile, off until switched on.
            assert!(!c.default_on && c.always.is_none(), "{name}");
        }
        let odd = with(ZV1_CA, 0, 21);
        let c = read(&sony_file(&ZV1_VIG, &odd, &ZV1_DIST)).unwrap();
        assert!(c.chromatic.is_none(), "odd CA count");
    }
}
