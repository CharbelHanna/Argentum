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
//! - The 33 radii are `fit::knots`, the ones the other readers sample at.
//!
//! RAWmakase `52da1bf` (MIT), the same file: an opcode is centred on its own
//! optical centre (cx, cy), with its radius 1 at the corner of the image it
//! applies to farthest from that centre, as the DNG SDK normalizes it, and
//! one centre serves the correction: the warp's, a vignette centred
//! elsewhere being left out. Here that image, the active area, is
//! `developed_frame`'s decoded image, and the centre and radii are moved
//! into the developed image, its DefaultCrop, a few pixels smaller on the
//! files seen. Without the frame, only a centred opcode is read, its radius
//! taken as the developed image's.

use rawler::tags::DngTag;

use super::fit::knots;
use super::{DevelopedFrame, LensCorrection, Radial};

/// DNG opcode IDs (DNG Specification 1.7.1.0, chapter 7).
const WARP_RECTILINEAR: u32 = 1;
const FIX_VIGNETTE_RADIAL: u32 = 3;

/// A DNG's FixVignetteRadial and WarpRectilinear; `None` for a file that is
/// not a DNG or has neither. `frame` places the developed image in the
/// active area the opcodes apply to.
pub(super) fn read(
    file: &[u8],
    frame: &dyn Fn() -> Option<DevelopedFrame>,
) -> Option<LensCorrection> {
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
    let vignetting_op = [DngTag::OpcodeList2, DngTag::OpcodeList3]
        .into_iter()
        .find_map(|tag| first(tag, FIX_VIGNETTE_RADIAL));
    let warp_op = first(DngTag::OpcodeList3, WARP_RECTILINEAR);
    if vignetting_op.is_none() && warp_op.is_none() {
        return None;
    }
    let frame = frame();
    let vignetting = vignetting_op.and_then(|p| vignette(&p, frame));
    let warp = warp_op.and_then(|p| warp(&p, frame));
    // RAWmakase 52da1bf: crates/rawmakase-model/src/dng.rs
    // One centre for the correction: the warp's, which moves the image. A vignette
    // centred elsewhere (no file seen) is left out rather than moved.
    let centre = match (&warp, &vignetting) {
        (Some((_, _, c)), _) | (None, Some((_, c))) => *c,
        (None, None) => LensCorrection::IMAGE_CENTRE,
    };
    let size = frame.map(|f| [f.crop[2] as f32, f.crop[3] as f32]);
    let vignetting = vignetting
        .filter(|(_, c)| size.is_none_or(|size| same_centre(*c, centre, size)))
        .map(|(v, _)| v);
    let (distortion, chromatic) = warp.map_or((None, None), |(d, c, _)| (d, c));
    let lens = LensCorrection {
        source: "DNG".into(),
        default_on: true,
        vignetting,
        distortion,
        chromatic,
        always: None,
        centre: Some(centre),
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

/// An opcode's optical centre (`cx`, `cy`, fractions of the active area),
/// moved into the developed image, and the factor from our radius, 1 at the
/// developed image's half diagonal, to the opcode's, 1 at the active area's
/// corner farthest from that centre (the DNG SDK's normalization), with our
/// knots out to the developed image's farthest corner. Without the frame
/// only a centred opcode is read.
// RAWmakase 52da1bf: crates/rawmakase-model/src/dng.rs (`opcode_frame`)
fn opcode_frame(
    cx: f64,
    cy: f64,
    frame: Option<DevelopedFrame>,
) -> Option<([f32; 2], f64, Vec<f32>)> {
    let Some(DevelopedFrame { decoded, crop }) = frame else {
        return ((cx - 0.5).abs() < 0.02 && (cy - 0.5).abs() < 0.02)
            .then(|| (LensCorrection::IMAGE_CENTRE, 1., knots(1.)));
    };
    if ![cx, cy].iter().all(|c| (0. ..=1.).contains(c)) {
        return None;
    }
    let [aw, ah] = decoded.map(|v| v as f64);
    let [left, top, w, h] = crop.map(|v| v as f64);
    let (x, y) = (cx * aw, cy * ah);
    let unit = x.max(aw - x).hypot(y.max(ah - y));
    let half = w.hypot(h) * 0.5;
    if !(unit > 0. && half > 0.) {
        return None;
    }
    let centre = [((x - left) / w) as f32, ((y - top) / h) as f32];
    if !centre.iter().all(|c| (0. ..=1.).contains(c)) {
        return None;
    }
    let reach = super::reach(centre, [w as f32, h as f32]);
    Some((centre, half / unit, knots(reach)))
}

/// Two optical centres within half a pixel of each other on an image of
/// `size` pixels.
// RAWmakase 52da1bf: crates/rawmakase-model/src/dng.rs
fn same_centre(a: [f32; 2], b: [f32; 2], size: [f32; 2]) -> bool {
    ((a[0] - b[0]) * size[0]).hypot((a[1] - b[1]) * size[1]) <= 0.5
}

/// FixVignetteRadial: gain 1 + k0 r^2 + k1 r^4 + k2 r^6 + k3 r^8 + k4 r^10,
/// and its centre.
fn vignette(p: &[u8], frame: Option<DevelopedFrame>) -> Option<(Radial, [f32; 2])> {
    let v = doubles(p);
    if v.len() != 7 {
        return None;
    }
    let (centre, to_opcode, knots) = opcode_frame(v[5], v[6], frame)?;
    let values = knots
        .iter()
        .map(|r| {
            let r2 = (f64::from(*r) * to_opcode).powi(2);
            (1. + (0..5).map(|i| v[i] * r2.powi(i as i32 + 1)).sum::<f64>()) as f32
        })
        .collect();
    let r = Radial { knots, values };
    r.values
        .iter()
        .any(|g| (g - 1.).abs() > 1e-4)
        .then_some((r, centre))
}

/// The distortion and lateral CA of a WarpRectilinear warp, whose source
/// radius ratio is kr0 + kr1 r^2 + kr2 r^4 + kr3 r^6 per plane, and its
/// centre. Tangential terms are ignored.
type Warp = (Option<Radial>, Option<[Radial; 2]>, [f32; 2]);

fn warp(p: &[u8], frame: Option<DevelopedFrame>) -> Option<Warp> {
    let planes = u32::from_be_bytes(p.get(..4)?.try_into().ok()?) as usize;
    let v = doubles(p.get(4..)?);
    if !matches!(planes, 1 | 3) || v.len() != planes * 6 + 2 {
        return None;
    }
    let (centre, to_opcode, knots) = opcode_frame(v[planes * 6], v[planes * 6 + 1], frame)?;
    let ratio = |plane: usize, r: f32| {
        let k = &v[plane * 6..plane * 6 + 4];
        let r2 = (f64::from(r) * to_opcode).powi(2);
        (k[0] + k[1] * r2 + k[2] * r2 * r2 + k[3] * r2 * r2 * r2) as f32
    };
    let green = if planes == 3 { 1 } else { 0 };
    let curve = |f: &dyn Fn(f32) -> f32| Radial {
        knots: knots.clone(),
        values: knots.iter().map(|r| f(*r)).collect(),
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
        centre,
    ))
}

#[cfg(test)]
mod tests {
    use super::{DevelopedFrame, vignette, warp};
    use crate::rm_lens_builtin::SUB_IFDS;
    use crate::rm_lens_builtin::test_files::{be, dji_file, tiff};
    // The registry's `read`, as the app calls it.
    use crate::rm_lens_builtin::{Amounts, LensCorrection, LensMap, read};

    #[test]
    fn decodes_vignette_and_warp_opcodes() {
        // DSCF7853's FixVignetteRadial from Lightroom's DNG (RAWmakase's test).
        let p = be(&[0.2599, 0.3424, 0.0630, 0.0346, -0.0300, 0.5, 0.5]);
        let (v, _) = vignette(&p, None).unwrap();
        assert!((v.eval(1.) - 1.67).abs() < 0.01);
        assert!((v.eval(0.5) - 1.087).abs() < 0.002);
        let mut w = 3u32.to_be_bytes().to_vec();
        w.extend(be(&[1.0002, 0.0005, 0., 0., 0., 0.]));
        w.extend(be(&[1., 0., 0., 0., 0., 0.]));
        w.extend(be(&[1., 0., 0., 0., 0., 0.]));
        w.extend(be(&[0.5, 0.5]));
        let (distortion, chromatic, _) = warp(&w, None).unwrap();
        assert!(distortion.is_none());
        let [red, blue] = chromatic.unwrap();
        assert!((red.eval(1.) - 1.0007).abs() < 1e-5);
        assert_eq!(blue.eval(1.), 1.);
        // Off-centre corrections need the image's size, to normalize the radius.
        assert!(vignette(&be(&[0.2, 0., 0., 0., 0., 0.3, 0.5]), None).is_none());
    }

    /// An optical centre off the image's: the radius is 1 at the image's
    /// corner farthest from it, as the DNG SDK normalizes it (Adobe's DNG of
    /// a Panasonic LX100, whose warp is centred 8 px left of its 4144 px
    /// active area's centre). RAWmakase's test (52da1bf), with a developed
    /// image that is the whole active area, then a DefaultCrop inside it.
    #[test]
    fn reads_off_centre_opcodes_about_their_centre() {
        let whole = |w: usize, h: usize| {
            Some(DevelopedFrame {
                decoded: [w, h],
                crop: [0, 0, w, h],
            })
        };
        let (cx, cy) = (0.4980694980694981, 0.5);
        let mut w = 1u32.to_be_bytes().to_vec();
        w.extend(be(&[0.99995, -0.19355, 0.07209, -0.01145, 0., 0.]));
        w.extend(be(&[cx, cy]));
        let (distortion, _, centre) = warp(&w, whole(4144, 3104)).unwrap();
        assert_eq!(centre, [cx as f32, cy as f32]);
        let distortion = distortion.unwrap();
        let half = 4144f64.hypot(3104.) / 2.;
        let far = (cx * 4144.).max((1. - cx) * 4144.).hypot(1552.);
        let opcode = |q: f64| 0.99995 - 0.19355 * q * q + 0.07209 * q.powi(4) - 0.01145 * q.powi(6);
        for px in [500., 1500., 2500., far] {
            let ours = f64::from(distortion.eval((px / half) as f32));
            assert!((ours - opcode(px / far)).abs() < 2e-4, "{px}: {ours}");
        }
        // Knots reach the far corner.
        assert!(f64::from(*distortion.knots.last().unwrap()) >= far / half - 1e-4);

        // A DefaultCrop 8 px in from each side: the centre moves with it, the
        // opcode's radius stays the active area's.
        let cropped = Some(DevelopedFrame {
            decoded: [4144, 3104],
            crop: [8, 8, 4128, 3088],
        });
        let (distortion, _, centre) = warp(&w, cropped).unwrap();
        let x = cx * 4144. - 8.;
        assert!(
            (f64::from(centre[0]) * 4128. - x).abs() < 1e-2,
            "{centre:?}"
        );
        assert!((f64::from(centre[1]) * 3088. - 1544.).abs() < 1e-2);
        let half = 4128f64.hypot(3088.) / 2.;
        for px in [500., 1500., 2500.] {
            let ours = f64::from(distortion.as_ref().unwrap().eval((px / half) as f32));
            assert!((ours - opcode(px / far)).abs() < 2e-4, "{px}: {ours}");
        }

        let p = be(&[0.3, 0.1, 0., 0., 0., 0.46, 0.53]);
        let (v, centre) = vignette(&p, whole(6000, 4000)).unwrap();
        assert_eq!(centre, [0.46, 0.53]);
        let far = (0.54f64 * 6000.).hypot(0.53 * 4000.);
        let half = 6000f64.hypot(4000.) / 2.;
        let at = f64::from(v.eval((far / half) as f32));
        assert!((at - 1.4).abs() < 1e-3, "{at}");
        // A centre outside the image is not one.
        assert!(vignette(&be(&[0.2, 0., 0., 0., 0., 1.3, 0.5]), whole(4144, 3104)).is_none());
    }

    #[test]
    fn reads_a_dngs_opcodes_with_the_vignetting_in_either_list() {
        let c = read(&dji_file()).unwrap();
        assert_eq!(c.source, "DNG");
        assert!(c.default_on);
        assert!((c.vignetting.as_ref().unwrap().eval(1.) - 1.67).abs() < 0.01);
        assert!((c.distortion.as_ref().unwrap().eval(1.) - 0.94).abs() < 1e-5);
        assert!(c.chromatic.is_none(), "one plane has no CA");
        // Read now, it is centred and framed by green's edge.
        assert_eq!(c.centre, Some(LensCorrection::IMAGE_CENTRE));
        assert_eq!(
            LensMap::new(&c, 300., 200., Amounts::FULL).fill,
            c.fill_scale(Amounts::FULL, [300., 200.])
        );
        // Without its DNGVersion it is not a DNG, and has no Sony tables either.
        let not_dng = tiff(&[
            vec![(SUB_IFDS, 4, 1, vec![0; 4])],
            vec![(51022, 7, 4, vec![0; 4])],
        ]);
        assert!(read(&not_dng).is_none());
    }
}
