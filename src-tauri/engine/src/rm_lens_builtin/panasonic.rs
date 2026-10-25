//! Panasonic's (and Leica's RWL) built-in lens correction, read from an RW2.
//!
//! Panasonic writes the camera's distortion and lateral CA corrections into
//! every RW2's IFD0:
//!
//! - 0x0119, sixteen signed shorts (ExifTool's DistortionInfo). Value 7's low
//!   four bits are 1 when the correction applies, value 12 is the radius
//!   unit N in pixels, and values 8, 4 and 11 over 2^15 are a, b and c of the
//!   camera's own correction, source radius s to output radius
//!   s·(1 + a s² + b s⁴ + c s⁶). Value 5 scales it: the output radius r
//!   samples the source radius that this maps to r·(1 + v5/2^15).
//! - 0x011B, 32 signed shorts. Value 14 is 256 when the table holds data (0
//!   for lenses without). Values 17, 16, 4 and 11 are four source radii in
//!   pixels, the last about the half diagonal; values 12, 23, 8 and 26 (red)
//!   and 27, 10, 20 and 29 (blue) are the channel's displacement from green
//!   at those radii in −1/1000 px, so red and blue sample green's source
//!   radius plus it. Adobe applies a smooth fit of it, and so does this. The
//!   other values, many of which change from shot to shot, are not used.
//!
//! Lightroom applies both whatever its profile setting (Adobe DNG Converter
//! writes them into its DNGs as their warp, or for newer bodies the
//! distortion as the "Camera Settings" profile), so the correction starts on,
//! and the parts the file stores are always the camera's. The CA applies
//! whether or not the distortion correction is on, as Adobe's does. No
//! vignetting: Lightroom corrects none from the file.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase (MIT), `crates/rawmakase-model/src/lens/embedded.rs`:
//! - @ af35e25: the Panasonic distortion reader and its tests.
//! - @ d04ef4f: its lateral CA (`panasonic_ca`, `shorts`), the reader as
//!   that commit leaves it, and the tests it adds.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - IFD0 is read with rawler's TIFF reader in place of `rawmakase-tiff`,
//!   from the file's bytes rather than its path.
//! - Radius 1 is the developed image's half diagonal, as in every reader
//!   here: rawler crops an RW2 to its default crop, where RAWmakase's radius
//!   is over the decoded image; the CA's fit runs out to that half diagonal.
//!   The model's own unit (value 12) is in pixels, so the correction in
//!   pixels is the same.
//! - `always` names the parts the file stores.

use rawler::bits::Endian;
use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{Entry, GenericTiffReader, Value};

use super::LensCorrection;
use super::fit::{make, maker_model, solve4};

/// IFD0's DistortionInfo and lateral CA table.
pub(super) const PANASONIC_DISTORTION: u16 = 0x0119;
pub(super) const PANASONIC_CHROMATIC: u16 = 0x011b;

// RAWmakase d04ef4f: crates/rawmakase-model/src/lens/embedded.rs
/// The values of an IFD0 entry of `N` signed shorts, in the file's byte order.
fn shorts<const N: usize>(e: &Entry, endian: Endian) -> Option<[f64; N]> {
    let v: Vec<f64> = match &e.value {
        Value::Undefined(b) => (b.get(..2 * N)?.as_chunks::<2>().0.iter())
            .map(|c| f64::from(endian.read_u16(c, 0) as i16))
            .collect(),
        Value::SShort(s) => s.get(..N)?.iter().map(|x| f64::from(*x)).collect(),
        Value::Short(s) => s.get(..N)?.iter().map(|x| f64::from(*x as i16)).collect(),
        _ => return None,
    };
    v.try_into().ok()
}

// RAWmakase d04ef4f: crates/rawmakase-model/src/lens/embedded.rs
/// Panasonic's lateral CA (IFD0 0x011B) as red's and blue's scale relative to green,
/// each a function of green's source radius in pixels. `half` is the developed
/// image's half diagonal.
fn panasonic_ca(v: [f64; 32], half: f64) -> Option<[impl Fn(f64) -> f64; 2]> {
    let knots = [v[17], v[16], v[4], v[11]];
    if v[14] != 256. || knots[0] <= 0. || !knots.is_sorted_by(|a, b| a < b) || half <= 0. {
        return None;
    }
    let at = |i: [usize; 4]| i.map(|i| -v[i] / 1000.);
    let (red, blue) = (at([12, 23, 8, 26]), at([27, 10, 20, 29]));
    if red.iter().chain(&blue).all(|d| *d == 0.) {
        return None;
    }
    // Displacement in pixels at a source radius: straight lines between the knots,
    // from none at the centre, held past the last knot.
    let lines = |d: [f64; 4], g: f64| {
        let i = knots.iter().position(|k| g < *k).unwrap_or(4);
        let (k0, d0) = if i == 0 {
            (0., 0.)
        } else {
            (knots[i - 1], d[i - 1])
        };
        match knots.get(i) {
            Some(k1) => d0 + (d[i] - d0) * (g - k0) / (k1 - k0),
            None => d0,
        }
    };
    // Adobe applies a smooth fit of those lines in its warp's form, 1 + c0 + c1 s² +
    // c2 s⁴ + c3 s⁶ with s = g / half. Least squares with residuals weighted by s, out
    // to the half diagonal, matches the CA in its DNGs of 309 photos from 111 bodies
    // to within 0.15 px (median 0.015 px); the lines themselves differ by up to 0.86 px.
    let fit = |d: [f64; 4]| {
        let mut normal = [[0.; 5]; 4];
        for i in 1..=256 {
            let s = f64::from(i) / 256.;
            let s2 = s * s;
            let row = [s, s * s2, s * s2 * s2, s * s2 * s2 * s2];
            let y = lines(d, s * half) / half;
            for (j, n) in normal.iter_mut().enumerate() {
                for k in 0..4 {
                    n[k] += row[j] * row[k];
                }
                n[4] += row[j] * y;
            }
        }
        let c = solve4(normal)?;
        Some(move |g: f64| {
            let s2 = (g / half).powi(2);
            1. + c[0] + s2 * (c[1] + s2 * (c[2] + s2 * c[3]))
        })
    };
    Some([fit(red)?, fit(blue)?])
}

/// The corrections in an RW2's IFD0; `None` for a file that is not an RW2,
/// or one that stores neither. `size_px` is the developed image's.
pub(super) fn read(file: &[u8], size_px: &dyn Fn() -> Option<[f64; 2]>) -> Option<LensCorrection> {
    if !file.starts_with(b"IIU\0") {
        return None;
    }
    let tiff = GenericTiffReader::new_with_buffer(file, 0, 0, Some(0)).ok()?;
    let ifd0 = tiff.root_ifd();
    // RAWmakase d04ef4f: crates/rawmakase-model/src/lens/embedded.rs
    let info = ifd0
        .get_entry(PANASONIC_DISTORTION)
        .and_then(|e| shorts::<16>(e, ifd0.endian));
    let ca = ifd0
        .get_entry(PANASONIC_CHROMATIC)
        .and_then(|e| shorts::<32>(e, ifd0.endian));
    let size = size_px()?;
    let half = size[0].hypot(size[1]) * 0.5;
    let ca = ca.and_then(|v| panasonic_ca(v, half));
    // Value 7's low four bits switch the distortion correction on.
    let info = info.filter(|v| (v[7] as i16 as u16) & 0xf == 1 && v[12] > 0.);
    // Radii in the distortion model's unit, or in pixels without it.
    let unit = info.map_or(1., |v| v[12]);
    let [a, b, c] = info.map_or([0.; 3], |v| [v[8], v[4], v[11]].map(|x| x / 32768.));
    let scale = info.map_or(1., |v| 1. + v[5] / 32768.);
    // The camera's correction maps the source radius to the output radius; its
    // inverse, by Newton's method, is green's source radius for an output radius.
    let forward = |s: f64| {
        let s2 = s * s;
        s * (1. + s2 * (a + s2 * (b + s2 * c)))
    };
    let slope = |s: f64| {
        let s2 = s * s;
        1. + s2 * (3. * a + s2 * (5. * b + s2 * 7. * c))
    };
    let green = |r: f64| {
        let target = r * scale;
        let mut s = target;
        for _ in 0..50 {
            let d = slope(s);
            if d <= 0. {
                return None;
            }
            s -= (forward(s) - target) / d;
        }
        ((forward(s) - target).abs() < 1e-9 && s > 0.).then_some(s)
    };
    // The CA applies at green's source radius, in the sensor's pixels.
    let [red, blue] = match &ca {
        Some([red, blue]) => [Some(red), Some(blue)],
        None => [None, None],
    };
    let red = |rho: f64| red.map_or(1., |f| f(rho * unit));
    let blue = |rho: f64| blue.map_or(1., |f| f(rho * unit));
    let chromatic = ca.is_some().then_some([&red as &dyn Fn(f64) -> f64, &blue]);
    if info.is_none() && chromatic.is_none() {
        return None;
    }
    let (distortion, chromatic) = maker_model(size, unit, &green, chromatic);
    let source = if make(ifd0).starts_with("leica") {
        "Leica built-in"
    } else {
        "Panasonic built-in"
    };
    let mut c = LensCorrection {
        source: source.into(),
        default_on: true,
        vignetting: None,
        distortion,
        chromatic,
        always: None,
    };
    c.always = Some(c.held_parts());
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::{PANASONIC_CHROMATIC, PANASONIC_DISTORTION};
    use crate::rm_lens_builtin::Parts;
    use crate::rm_lens_builtin::test_files::{
        adobe, digest, directory, largest_ca_difference, largest_difference, read_framed, shorts,
    };

    /// An RW2 with DistortionInfo `info`.
    fn rw2(info: &[i16]) -> Vec<u8> {
        rw2_with(Some(info), None)
    }

    /// An RW2 with DistortionInfo `info` and the lateral CA table `ca` (0x011B).
    fn rw2_with(info: Option<&[i16]>, ca: Option<&[i16]>) -> Vec<u8> {
        let mut out = b"IIU\0\x08\0\0\0".to_vec();
        let mut entries = vec![(0x010f, 2, 10, b"Panasonic\0".to_vec())];
        for (tag, v) in [(PANASONIC_DISTORTION, info), (PANASONIC_CHROMATIC, ca)] {
            if let Some(v) = v {
                entries.push((tag, 7, 2 * v.len() as u32, shorts(v)));
            }
        }
        out.extend(directory(8, &entries));
        out
    }

    /// gx9_3_2.RW2 (raw.pixls.us, DC-GX9, LUMIX G 25/F1.7): its DistortionInfo, against
    /// the green plane of the warp Adobe DNG Converter 18.7 writes for it (radius 1 at
    /// the 5208 × 3904 image's corner farthest from (2600, 1952)).
    const GX9: [i16; 16] = [
        -24389, -25169, 545, 0, 652, 0, 211, 1, 447, 600, 434, -38, 3240, 1100, -30428, 8502,
    ];

    /// The default crop of gx9_3_2.RW2's 5200 × 3904 decoded image.
    const GX9_SIZE: [f64; 2] = [5184., 3888.];

    #[test]
    fn reads_panasonic_distortion_as_adobe_does() {
        let c = read_framed(&rw2(&GX9), GX9_SIZE).unwrap();
        assert!(c.default_on && c.chromatic.is_none() && c.vignetting.is_none());
        assert_eq!(c.source, "Panasonic built-in");
        assert_eq!(
            c.always,
            Some(Parts {
                distortion: true,
                ..Parts::default()
            })
        );
        let green = [
            1.0000012404328427,
            -0.013760663120671701,
            -0.02015800015496616,
            0.004736213335729822,
        ];
        let unit = 2608f64.hypot(1952.);
        let off = largest_difference(&c, GX9_SIZE, &|_, px| adobe(green, unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
    }

    /// The DMC-GM5's DistortionInfo layout: a scale in value 5 (P1080764.RW2,
    /// raw.pixls.us, 3:2 of a 4:3 sensor), against the green plane of Adobe's warp.
    #[test]
    fn reads_panasonic_distortion_scale() {
        let gm5 = [
            25348, 2828, 62, 619, 0, -32, 0, -4095, 51, 628, 516, -19, 2870, 1128, -17663, -1541,
        ];
        // The default crop of the 4608 × 3080 decoded image.
        let size = [4592., 3064.];
        let c = read_framed(&rw2(&gm5), size).unwrap();
        let green = [
            0.9990234634717745,
            -0.0014475342840505334,
            9.97146343806321e-06,
            0.00045846619652323284,
        ];
        let unit = 2304f64.hypot(1540.);
        let off = largest_difference(&c, size, &|_, px| adobe(green, unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
    }

    /// Raw.pixls.us photos: their DistortionInfo and lateral CA tags, the developed
    /// image's size (the decoded image's default crop), and the red, green and blue
    /// planes of the warp Adobe DNG Converter 18.7 writes for them, whose radius 1 is
    /// the farthest corner of the DNG's image from its optical centre.
    struct PanasonicCa {
        info: [i16; 16],
        ca: [i16; 32],
        size: [f64; 2],
        warp: [[f64; 4]; 3],
        /// The corner's offsets from the centre, in pixels.
        corner: [f64; 2],
    }

    /// gx9_3_2.RW2 (DC-GX9, LUMIX G 25/F1.7).
    const GX9_CA: PanasonicCa = PanasonicCa {
        info: GX9,
        ca: [
            -30167, -1805, 16569, 17920, 2730, 1024, 128, 21439, -246, 2184, -170, 3276, -473,
            18937, 256, 21168, 2184, 1092, 18608, 16640, -738, 0, -22016, -464, 2760, -464, 643,
            189, 17408, -2007, 17139, 15825,
        ],
        size: GX9_SIZE,
        warp: [
            [
                1.0004682282802129,
                -0.01451169102033667,
                -0.019598127970462897,
                0.004331191680153168,
            ],
            [
                1.0000012404328427,
                -0.013760663120671701,
                -0.02015800015496616,
                0.004736213335729822,
            ],
            [
                0.9998048878471808,
                -0.01311285159727582,
                -0.0202450377845782,
                0.004896631126171336,
            ],
        ],
        corner: [2608., 1952.],
    };

    #[test]
    fn reads_panasonic_chromatic_aberration_as_adobe_does() {
        // P1000403.RW2 (DMC-G80, SIGMA 60mm F2.8 DN): distortion correction off.
        let g80 = PanasonicCa {
            info: [
                8820, 0, 0, 0, 0, 0, 0, 0, 0, 0, 516, 0, 2870, 0, -12432, -15201,
            ],
            ca: [
                -2493, -15813, 1160, 12032, 2407, 256, 0, 14334, -633, 1408, 748, 2888, -230, -19,
                256, 1536, 1926, 963, 1376, 8320, 953, 96, 22912, -485, 10182, -485, -806, 361,
                512, 1175, -843, -17923,
            ],
            // The default crop of the 4608 × 3464 decoded image.
            size: [4592., 3448.],
            warp: [
                [
                    1.0002378144655706,
                    2.9725538808200724e-05,
                    9.90484904583866e-06,
                    1.8099946004213052e-06,
                ],
                [1., 0., 0., 0.],
                [
                    0.9996276377798108,
                    -4.8152284437108506e-05,
                    3.178458018560093e-05,
                    -1.8260554076832136e-05,
                ],
            ],
            corner: [2304., 1732.],
        };
        // P1000416.RW2 (DC-TZ202): strong distortion, CA knots at 2/7, 4/7 and 6/7 of
        // the radius, and up to 3.9 px of CA. Adobe's polynomial departs from the
        // straight lines between the knots by up to 0.85 px here.
        let tz202 = PanasonicCa {
            info: [
                24343, 1775, 408, 0, 1460, 0, 631, 1, 6781, 189, 508, 587, 3288, 689, 24841, -25795,
            ],
            ca: [
                -18496, 12507, 4424, -25296, 2808, 384, -2560, 13819, 234, 832, -963, 3276, -586,
                -26642, 256, 8520, 1872, 936, -32280, 4416, -3385, 2112, 0, -859, -14384, -347,
                3085, -325, -22528, -8437, -18839, 17586,
            ],
            // The default crop of the 5488 × 3664 decoded image.
            size: [5472., 3648.],
            warp: [
                [
                    0.9998364018762427,
                    -0.2003806248746034,
                    0.05753545636529678,
                    -0.00963193389561301,
                ],
                [
                    0.9991955093872854,
                    -0.1997130260581652,
                    0.057697840880385876,
                    -0.009660068321890813,
                ],
                [
                    0.9995507528793499,
                    -0.19942467462750102,
                    0.05834188670640916,
                    -0.009773256243380768,
                ],
            ],
            corner: [2744., 1832.],
        };
        for (name, case) in [("GX9", &GX9_CA), ("G80", &g80), ("TZ202", &tz202)] {
            let c = read_framed(&rw2_with(Some(&case.info), Some(&case.ca)), case.size).unwrap();
            assert!(c.chromatic.is_some(), "{name}");
            let unit = case.corner[0].hypot(case.corner[1]);
            let adobe = |ch: usize, px| adobe(case.warp[ch], unit, px);
            let (off, ca) = largest_ca_difference(&c, case.size, &adobe);
            assert!(ca > 1., "{name}: Adobe corrects {ca} px");
            assert!(off < 0.1, "{name}: {off} px from Adobe's CA");
        }
        // The G80's lens needs no distortion correction, only the CA.
        let c = read_framed(&rw2_with(Some(&g80.info), Some(&g80.ca)), g80.size).unwrap();
        assert!(c.distortion.is_none());
        assert_eq!(
            c.always,
            Some(Parts {
                chromatic: true,
                ..Parts::default()
            })
        );
    }

    /// Newer bodies, whose DNGs carry the distortion in an embedded profile, keep the CA
    /// in the warp, on the sensor's radius: P1002296.RW2 (DC-GH7, LEICA DG 12-35/F2.8 at
    /// 12mm).
    #[test]
    fn reads_panasonic_chromatic_aberration_on_the_sensor_radius() {
        let info = [
            18216, -26703, 241, 0, -399, 0, 124, 1, 4495, 436, 354, -1062, 3611, 936, -23617,
            -17367,
        ];
        let ca = [
            27551, 1983, 18, 15368, 3032, 32, -5888, 6654, -1268, 338, -546, 3638, -725, -1, 256,
            2064, 2426, 1213, 18128, 0, -1086, 384, -3584, -1268, -4230, -1268, -676, -159, -17152,
            -2148, -15798, -17015,
        ];
        let warp = [
            [
                1.000588699784562,
                -2.5632476810422133e-05,
                -0.00029661560319560465,
                -8.09482177484e-05,
            ],
            [1., 0., 0., 0.],
            [
                1.0001421369447667,
                1.0062062321125609e-05,
                0.0005116995869975938,
                -7.363913965363622e-05,
            ],
        ];
        // The default crop of the 5792 × 4352 decoded image.
        let size = [5776., 4336.];
        let c = read_framed(&rw2_with(Some(&info), Some(&ca)), size).unwrap();
        assert!(c.distortion.is_some());
        let half = size[0].hypot(size[1]) / 2.;
        // Adobe's warp applies at the sensor radius green samples after distortion.
        let sensor = |px: f64| {
            px * f64::from(
                c.radial_scale_with((px / half) as f32, crate::rm_lens_builtin::Amounts::FULL)[1],
            )
        };
        let unit = 2896f64.hypot(2176.);
        let (off, most) =
            largest_ca_difference(&c, size, &|ch, px| adobe(warp[ch], unit, sensor(px)));
        assert!(most > 1., "Adobe corrects {most} px");
        assert!(off < 0.1, "{off} px from Adobe's CA");
    }

    /// Lenses without CA data store an empty table; malformed tables are not read.
    #[test]
    fn panasonic_chromatic_tables_without_data_are_not_read() {
        let size = GX9_CA.size;
        let mut empty = [0; 32];
        // A DMC-G6's: only the values that change with each shot are set.
        [empty[0], empty[1], empty[2], empty[7], empty[13], empty[31]] =
            [-5508, 3276, 4624, -26724, 24178, 852];
        let mut unordered = GX9_CA.ca;
        unordered.swap(16, 17);
        let mut unknown = GX9_CA.ca;
        unknown[14] = 512;
        for (name, ca) in [
            ("empty", &empty[..]),
            ("unordered", &unordered[..]),
            ("unknown layout", &unknown[..]),
            ("short", &GX9_CA.ca[..31]),
        ] {
            let c = read_framed(&rw2_with(Some(&GX9), Some(ca)), size).unwrap();
            assert!(c.chromatic.is_none() && c.distortion.is_some(), "{name}");
            assert!(
                read_framed(&rw2_with(None, Some(ca)), size).is_none(),
                "{name}"
            );
        }
        // Without DistortionInfo the CA is still read.
        let c = read_framed(&rw2_with(None, Some(&GX9_CA.ca)), size).unwrap();
        assert!(c.chromatic.is_some() && c.distortion.is_none());
    }

    /// A model whose correction folds back on itself has no inverse to sample.
    #[test]
    fn panasonic_model_without_inverse_is_not_read() {
        let mut folding = GX9;
        folding[11] = -32768;
        assert!(read_framed(&rw2(&folding), GX9_SIZE).is_none());
    }

    #[test]
    fn panasonic_correction_switched_off_is_not_read() {
        let mut off = GX9;
        off[7] = 0;
        assert!(read_framed(&rw2(&off), GX9_SIZE).is_none());
    }

    /// Today's reader, pinned as `todays_readers_are_pinned` pins the others: a
    /// change meant to move it updates the number; any other change fails.
    #[test]
    fn todays_reader_is_pinned() {
        let file = rw2_with(Some(&GX9_CA.info), Some(&GX9_CA.ca));
        assert_eq!(
            digest(&read_framed(&file, GX9_SIZE)),
            14_116_556_787_168_533_122
        );
    }
}
