//! Sony's built-in lens correction, read from an ARW.
//!
//! Sony writes distortion, lateral chromatic aberration and vignetting into
//! an ARW's raw SubIFD (0x7032 vignetting, 0x7035 CA, 0x7037 distortion).
//! Lightroom has not been shown to apply them, so they start off. Sony's
//! tables carry the camera's own framing (a ZV-1's distortion is 1.077 at
//! the centre), so they are filled only as far as the photo's edge needs,
//! which keeps the camera's crop (`LensMap`'s fill, in `rm_lens_builtin.rs`).
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `v0.4.0` @ a8c5d6d7 (MIT):
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Sony reader, its
//!   units, and its tests.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The tables are read with rawler's TIFF reader in place of
//!   `rawmakase-tiff`, and from the file's bytes rather than its path.

use std::io::Cursor;

use rawler::formats::tiff::IFD;

use super::{LensCorrection, Radial};

/// SubIFDs: an ARW's raw image directory is the first.
pub(super) const SUB_IFDS: u16 = 0x14a;
pub(super) const SONY_VIGNETTING: u16 = 0x7032;
pub(super) const SONY_CHROMATIC: u16 = 0x7035;
pub(super) const SONY_DISTORTION: u16 = 0x7037;

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
    let sub = root.get_sub_ifd_all(SUB_IFDS)?.first()?;
    let numbers = |tag: u16| -> Option<Vec<f32>> {
        let v = &sub.get_entry(tag)?.value;
        (0..v.count())
            .map(|i| v.get_f32(i).ok().flatten())
            .collect()
    };
    let knots = |n: usize| {
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
        Radial::new(knots(v.len()), v.iter().map(|x| f(*x)).collect())
    };
    let vignetting = single(numbers(SONY_VIGNETTING), &|v| {
        (2f32.powf(v / 8192. - 1.) - 0.5).exp2()
    })
    .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-4));
    let distortion = single(numbers(SONY_DISTORTION), &|d| 1. + d / 16384.)
        .filter(|r| r.values.iter().any(|v| (v - 1.).abs() > 1e-6));
    let chromatic = counted(numbers(SONY_CHROMATIC), 2).and_then(|v| {
        let n = v.len() / 2;
        let scale = |s: &[f32]| s.iter().map(|x| 1. + x / 2_097_152.).collect();
        Some([
            Radial::new(knots(n), scale(&v[..n]))?,
            Radial::new(knots(n), scale(&v[n..]))?,
        ])
    });
    Some(LensCorrection {
        source: LensCorrection::SONY_BUILTIN.into(),
        // Not yet compared with Lightroom, which may apply its own profile instead.
        default_on: false,
        vignetting,
        distortion,
        chromatic,
        always: None,
    })
}

#[cfg(test)]
mod tests {
    use crate::rm_lens_builtin::test_files::{ZV1_CA, ZV1_DIST, ZV1_VIG, sony_file};
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::{LensCorrection, read};

    #[test]
    fn reads_sony_subifd_tables() {
        let mut vig = vec![16i16];
        vig.extend((0..16).map(|i| i * 800));
        let mut dist = vec![16i16];
        dist.extend((0..16).map(|i| i * 10));
        let mut ca = vec![32i16];
        ca.extend(std::iter::repeat_n(-384, 16));
        ca.extend(std::iter::repeat_n(0, 16));
        let c = read(&sony_file(&vig, &ca, &dist)).unwrap();
        assert!(!c.default_on);
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
        assert_eq!(dist.knots.len(), 11);
        assert_eq!(dist.knots[1], 0.1);
        assert!((dist.eval(0.) - (1. + 1265. / 16384.)).abs() < 1e-6);
        assert!((dist.eval(1.) - (1. - 1725. / 16384.)).abs() < 1e-6);
        let [red, blue] = c.chromatic.as_ref().unwrap();
        assert_eq!(red.knots.len(), 11);
        assert!((red.eval(1.) - (1. + 1408. / 2_097_152.)).abs() < 1e-7);
        assert!((blue.eval(0.1) - (1. + 1280. / 2_097_152.)).abs() < 1e-7);
        assert!((blue.eval(1.) - (1. + 256. / 2_097_152.)).abs() < 1e-7);
        assert_eq!(c.vignetting.as_ref().unwrap().knots.len(), 16);
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
        }
        let odd = with(ZV1_CA, 0, 21);
        let c = read(&sony_file(&ZV1_VIG, &odd, &ZV1_DIST)).unwrap();
        assert!(c.chromatic.is_none(), "odd CA count");
    }
}
