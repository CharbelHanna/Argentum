//! Fujifilm's built-in lens correction, read from a RAF.
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
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Fujifilm reader, its
//!   units, the 0.85 vignetting strength, and its tests.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The tables are read with rawler's TIFF reader, as `raf.rs` reads the
//!   FujiIFD, in place of `rawmakase-tiff`, and from the file's bytes rather
//!   than its path.

use std::io::Cursor;

use rawler::formats::tiff::IFD;

use super::{LensCorrection, Radial};

/// FujiIFD, in the TIFF a RAF's header points to.
pub(super) const FUJI_IFD: u16 = 0xf000;
pub(super) const FUJI_DISTORTION: u16 = 0xf00b;
pub(super) const FUJI_CHROMATIC: u16 = 0xf00f;
pub(super) const FUJI_VIGNETTING: u16 = 0xf010;
/// Where a RAF's header holds the offset of that TIFF, big-endian.
pub(super) const RAF_TIFF_POINTER: usize = 100;

/// Lightroom renders Fujifilm's vignetting table about 15% weaker in log
/// gain: corners of three X100F photos match Camera Raw at this exponent
/// (+0.08 EV at the corner at full strength). RAWmakase's measurement.
const FUJI_VIGNETTE_STRENGTH: f32 = 0.85;

/// Fujifilm's tables are signed rationals. The first value is a pixel scale;
/// knots are fractions of the half diagonal, distortion is in percent, CA a
/// radius fraction, vignetting the remaining illumination in percent.
/// `None` for a file that is not a RAF, or one whose FujiIFD is not found.
pub(super) fn read(file: &[u8]) -> Option<LensCorrection> {
    if !file.starts_with(b"FUJIFILMCCD-RAW") {
        return None;
    }
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
        always: None,
    })
}

#[cfg(test)]
mod tests {
    use super::FUJI_VIGNETTE_STRENGTH;
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::read;
    use crate::rm_lens_builtin::test_files::fuji_file;

    #[test]
    fn reads_fujifilm_raf_tables() {
        let c = read(&fuji_file()).unwrap();
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
}
