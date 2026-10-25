//! A DNG's lens correction, read from its opcodes.
//!
//! A DNG records the corrections Lightroom applies as opcodes:
//! FixVignetteRadial (a gain over radius) and WarpRectilinear (a radial
//! remap per colour plane). Cameras that write DNGs (Leica, DJI's drones,
//! phones) and Lightroom's own DNGs carry them. On, as Lightroom applies
//! them.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase `v0.4.0` @ a8c5d6d7 (MIT):
//! - `crates/rawmakase-model/src/dng.rs`: the FixVignetteRadial and
//!   WarpRectilinear readers (radial terms only, centred within 0.02,
//!   sampled at 33 radii), and their test.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - FixVignetteRadial is read from OpcodeList2 and, when that has none,
//!   from OpcodeList3: DJI's drones write it there, where RAWmakase does not
//!   look. Both lists apply before the warp, and a gain is the same in
//!   either. The lists come through `dng_opcodes.rs`, shared with the gain
//!   maps.
//! - A DNG's `source` is "DNG", the word the switch shows, where RAWmakase
//!   writes "DNG opcodes".
//! - A DNG's radius is over its raw image; the developed image here is its
//!   DefaultCrop, a few pixels smaller on the files seen.

use rawler::tags::DngTag;

use super::{LensCorrection, Radial};

/// DNG opcode IDs (DNG Specification 1.7.1.0, chapter 7).
const WARP_RECTILINEAR: u32 = 1;
const FIX_VIGNETTE_RADIAL: u32 = 3;

/// A DNG's FixVignetteRadial and WarpRectilinear; `None` for a file that is
/// not a DNG or has neither.
pub(super) fn read(file: &[u8]) -> Option<LensCorrection> {
    if !super::is_tiff(file) {
        return None;
    }
    let first = |tag: DngTag, id: u32| -> Option<Vec<u8>> {
        let list = crate::dng_opcodes::list(file, tag)?;
        let ops = crate::dng_opcodes::opcodes(&list)?;
        ops.into_iter()
            .find(|op| op.id == id)
            .map(|op| op.params.to_vec())
    };
    let vignetting = [DngTag::OpcodeList2, DngTag::OpcodeList3]
        .into_iter()
        .find_map(|tag| first(tag, FIX_VIGNETTE_RADIAL))
        .and_then(|p| vignette(&p));
    let (distortion, chromatic) = first(DngTag::OpcodeList3, WARP_RECTILINEAR)
        .and_then(|p| warp(&p))
        .unwrap_or((None, None));
    let lens = LensCorrection {
        source: "DNG".into(),
        default_on: true,
        vignetting,
        distortion,
        chromatic,
    };
    (!lens.is_empty()).then_some(lens)
}

fn doubles(p: &[u8]) -> Vec<f64> {
    p.as_chunks::<8>()
        .0
        .iter()
        .map(|c| f64::from_be_bytes(*c))
        .collect()
}

/// Radius samples; the opcode radius, like ours, is 1 at the farthest corner
/// from the optical centre, which must be near the image centre.
fn dng_knots() -> Vec<f32> {
    (0..=32).map(|i| i as f32 / 32.).collect()
}

fn centred(cx: f64, cy: f64) -> bool {
    (cx - 0.5).abs() < 0.02 && (cy - 0.5).abs() < 0.02
}

/// FixVignetteRadial: gain 1 + k0 r^2 + k1 r^4 + k2 r^6 + k3 r^8 + k4 r^10.
fn vignette(p: &[u8]) -> Option<Radial> {
    let v = doubles(p);
    if v.len() != 7 || !centred(v[5], v[6]) {
        return None;
    }
    let values = dng_knots()
        .iter()
        .map(|r| {
            let r2 = f64::from(*r * *r);
            (1. + (0..5).map(|i| v[i] * r2.powi(i as i32 + 1)).sum::<f64>()) as f32
        })
        .collect();
    let r = Radial {
        knots: dng_knots(),
        values,
    };
    r.values.iter().any(|g| (g - 1.).abs() > 1e-4).then_some(r)
}

/// WarpRectilinear: source radius ratio kr0 + kr1 r^2 + kr2 r^4 + kr3 r^6 per
/// plane. Tangential terms are ignored.
fn warp(p: &[u8]) -> Option<(Option<Radial>, Option<[Radial; 2]>)> {
    let planes = u32::from_be_bytes(p.get(..4)?.try_into().ok()?) as usize;
    let v = doubles(p.get(4..)?);
    if !matches!(planes, 1 | 3)
        || v.len() != planes * 6 + 2
        || !centred(v[planes * 6], v[planes * 6 + 1])
    {
        return None;
    }
    let ratio = |plane: usize, r: f32| {
        let k = &v[plane * 6..plane * 6 + 4];
        let r2 = f64::from(r * r);
        (k[0] + k[1] * r2 + k[2] * r2 * r2 + k[3] * r2 * r2 * r2) as f32
    };
    let green = if planes == 3 { 1 } else { 0 };
    let curve = |f: &dyn Fn(f32) -> f32| Radial {
        knots: dng_knots(),
        values: dng_knots().into_iter().map(f).collect(),
    };
    let distortion = curve(&|r| ratio(green, r));
    let chromatic = (planes == 3).then(|| {
        [
            curve(&|r| ratio(0, r) / ratio(1, r)),
            curve(&|r| ratio(2, r) / ratio(1, r)),
        ]
    });
    let changes = |r: &Radial| r.values.iter().any(|v| (v - 1.).abs() > 1e-6);
    Some((
        Some(distortion).filter(changes),
        chromatic.filter(|[a, b]| changes(a) || changes(b)),
    ))
}

#[cfg(test)]
mod tests {
    use super::{vignette, warp};
    use crate::rm_lens_builtin::sony::SUB_IFDS;
    use crate::rm_lens_builtin::test_files::{be, dji_file, tiff};
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::{Amounts, LensMap, read};

    #[test]
    fn decodes_vignette_and_warp_opcodes() {
        // DSCF7853's FixVignetteRadial from Lightroom's DNG (RAWmakase's test).
        let p = be(&[0.2599, 0.3424, 0.0630, 0.0346, -0.0300, 0.5, 0.5]);
        let v = vignette(&p).unwrap();
        assert!((v.eval(1.) - 1.67).abs() < 0.01);
        assert!((v.eval(0.5) - 1.087).abs() < 0.002);
        let mut w = 3u32.to_be_bytes().to_vec();
        w.extend(be(&[1.0002, 0.0005, 0., 0., 0., 0.]));
        w.extend(be(&[1., 0., 0., 0., 0., 0.]));
        w.extend(be(&[1., 0., 0., 0., 0., 0.]));
        w.extend(be(&[0.5, 0.5]));
        let (distortion, chromatic) = warp(&w).unwrap();
        assert!(distortion.is_none());
        let [red, blue] = chromatic.unwrap();
        assert!((red.eval(1.) - 1.0007).abs() < 1e-5);
        assert_eq!(blue.eval(1.), 1.);
        // Off-centre corrections are not representable and are skipped.
        assert!(vignette(&be(&[0.2, 0., 0., 0., 0., 0.3, 0.5])).is_none());
    }

    #[test]
    fn reads_a_dngs_opcodes_with_the_vignetting_in_either_list() {
        let c = read(&dji_file()).unwrap();
        assert_eq!(c.source, "DNG");
        assert!(c.default_on);
        assert!((c.vignetting.as_ref().unwrap().eval(1.) - 1.67).abs() < 0.01);
        assert!((c.distortion.as_ref().unwrap().eval(1.) - 0.94).abs() < 1e-5);
        assert!(c.chromatic.is_none(), "one plane has no CA");
        // The whole frame's corners, not Sony's camera frame.
        assert_eq!(
            LensMap::new(&c, 300., 200., Amounts::FULL).fill,
            c.fill_scale_with(Amounts::FULL)
        );
        // Without its DNGVersion it is not a DNG, and has no Sony tables either.
        let not_dng = tiff(&[
            vec![(SUB_IFDS, 4, 1, vec![0; 4])],
            vec![(51022, 7, 4, vec![0; 4])],
        ]);
        assert!(read(&not_dng).is_none());
    }
}
