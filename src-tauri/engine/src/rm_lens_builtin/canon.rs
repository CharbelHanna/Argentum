//! Canon's built-in lens correction, read from an EOS R5 Mark II's or R6
//! Mark III's CR3.
//!
//! Record 14 of a CR3's timed metadata (the `CTMD` track's sample) holds,
//! in little-endian shorts, three counts of knots, then seven arrays of 23
//! slots: vignetting (the light kept, in 2^-13) and its knots, red and blue
//! (their scale relative to green, 2^14 + 2^23·(s − 1)) and their knots,
//! distortion (green's source radius over the output radius, in 2^-14) and
//! its knots. Knots are radii in units of 5.314 µm on the sensor, from 0 at
//! the centre. The distortion is applied as Adobe fits it
//! (`canon_distortion`).
//!
//! Adobe DNG Converter turns it into a "Camera Settings" lens profile, which
//! Lightroom applies only with Enable Profile Corrections on, and for these
//! two bodies alone (an R50 V stores the same record). So it starts off, and
//! switching it on applies all of it, as a profile does.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase @ 37ea972 (MIT):
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Canon reader
//!   (`canon`, `canon_tables`, `canon_distortion`, the bodies and units),
//!   and its tests.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The CR3's boxes are read with rawler's BMFF reader (`formats::bmff`) in
//!   place of RAWmakase's box walker (`boxes`, `child`, `read_at`,
//!   `cr3_moov`, `ctmd_sample`): the model from Canon's CMT1, the sample
//!   from the CTMD track's first chunk offset (`co64`, which Canon writes;
//!   rawler does not read a 32-bit `stco`) and size. The file is in memory,
//!   so the sample is a slice of it, and RAWmakase's 4 MiB read limit has
//!   nothing to bound.
//! - Radius 1 is the developed image's half diagonal, as in every reader
//!   here, and the default crop whose corner the distortion keeps in place is
//!   the developed image: rawler crops a CR3 to its default crop.
//! - The test file carries the boxes rawler's reader requires around
//!   RAWmakase's (movie, track, media and handler headers, the sample
//!   table's time and chunk tables, Canon's other metadata boxes).

use rawler::formats::bmff::Bmff;
use rawler::formats::tiff::reader::TiffReader;
use rawler::tags::TiffCommonTag;

use super::fit::solve4;
use super::{LensCorrection, Radial};

// RAWmakase 37ea972: crates/rawmakase-model/src/lens/embedded.rs
/// The bodies whose CR3 lens tables Adobe applies, with the width in pixels of their
/// full-frame image, 36 mm. DNG Converter 18.7 uses the tables of no other body, not
/// even of an R50 V, which stores the same record.
const CANON_BODIES: [(&str, f64); 2] =
    [("Canon EOS R5m2", 8192.), ("Canon EOS R6 Mark III", 6960.)];
/// The unit of Canon's knots in millimetres: Adobe's piecewise vignetting puts them at
/// this many on both bodies, to 0.02%.
const CANON_KNOT_MM: f64 = 0.005314;
/// Adobe fits Canon's distortion table with its values limited to this.
const CANON_DISTORTION_MAX: f64 = 1.0275;
/// The timed metadata's record that holds the lens tables.
const LENS_RECORD: u16 = 14;

/// The tables in a CR3 of either body; `None` for a file that is not a CR3,
/// one of another body, or one without the record. `size_px` is the
/// developed image's.
pub(super) fn read(file: &[u8], size_px: &dyn Fn() -> Option<[f64; 2]>) -> Option<LensCorrection> {
    if file.get(4..12) != Some(b"ftypcrx ".as_slice()) {
        return None;
    }
    let moov = Bmff::new_buf(file).ok()?.filebox.moov;
    let model = moov
        .cr3desc
        .as_ref()?
        .cmt1
        .tiff
        .root_ifd()
        .get_entry(TiffCommonTag::Model)?
        .value
        .as_string()?
        .as_str();
    // RAWmakase 37ea972: crates/rawmakase-model/src/lens/embedded.rs
    let width = CANON_BODIES.iter().find(|(name, _)| *name == model)?.1;
    // The timed metadata's track and its one sample.
    let stbl = &moov
        .traks
        .iter()
        .find(|t| t.mdia.minf.stbl.stsd.ctmd.is_some())?
        .mdia
        .minf
        .stbl;
    let at = usize::try_from(*stbl.co64.as_ref()?.entries.first()?).ok()?;
    let len = match stbl.stsz.sample_size {
        0 => *stbl.stsz.sample_sizes.first()?,
        n => n,
    } as usize;
    let sample = file.get(at..at.checked_add(len)?)?;
    // RAWmakase 37ea972: crates/rawmakase-model/src/lens/embedded.rs
    // Records: a little-endian length and kind, six more bytes, the data.
    let mut at = 0usize;
    let tables = std::iter::from_fn(|| {
        let len = u32::from_le_bytes(sample.get(at..at + 4)?.try_into().ok()?) as usize;
        let r = sample.get(at..at.checked_add(len)?).filter(|_| len >= 12)?;
        at += len;
        Some((u16::from_le_bytes([r[4], r[5]]), &r[12..]))
    })
    .find(|(kind, _)| *kind == LENS_RECORD)?
    .1;
    canon_tables(tables, width * CANON_KNOT_MM / 36., size_px()?)
}

// RAWmakase 37ea972: crates/rawmakase-model/src/lens/embedded.rs
/// Record 14 of a CR3's timed metadata: three counts of knots, then seven arrays of
/// as many slots each: vignetting and its knots, red, blue and their knots,
/// distortion and its knots. `unit` is a knot in pixels; `size_px` the developed
/// image's.
fn canon_tables(t: &[u8], unit: f64, size_px: [f64; 2]) -> Option<LensCorrection> {
    let v: Vec<f64> = (t.as_chunks::<2>().0.iter())
        .map(|c| f64::from(u16::from_le_bytes(*c)))
        .collect();
    let (counts, arrays) = v.split_at_checked(3)?;
    let slots = arrays.len() / 7;
    if slots < 2 || arrays.len() != 7 * slots {
        return None;
    }
    let half = size_px[0].hypot(size_px[1]) * 0.5;
    if !(half > 0. && unit > 0.) {
        return None;
    }
    // The `n` tables from array `first` and the knots after them, from the centre out.
    let group = |count: f64, first: usize, n: usize| {
        let count = count as usize;
        if !(2..=slots).contains(&count) {
            return None;
        }
        let array = |i: usize| &arrays[i * slots..i * slots + count];
        let knots = array(first + n);
        (knots[0] == 0. && knots.is_sorted_by(|a, b| a < b))
            .then(|| (knots, (first..first + n).map(array).collect::<Vec<_>>()))
    };
    let radii = |knots: &[f64]| knots.iter().map(|k| (k * unit / half) as f32).collect();
    let changes = |r: &Radial, by: f32| r.values.iter().any(|v| (v - 1.).abs() > by);
    // The share of light kept, in 2^-13, straight lines between the knots as in Adobe's
    // piecewise profile. The gain restores it at more radii in each interval, as many as
    // Radial holds (two more for 17 knots): the RF135mm F1.8's falls to 0.24, and lines
    // through the gains alone would be 0.0027 off between the knots.
    let vignetting = group(counts[0], 0, 1)
        .filter(|(_, t)| t[0].iter().all(|v| *v > 0.))
        .and_then(|(k, t)| {
            let parts = (63 / (k.len() - 1)).min(3);
            let lines = k.windows(2).zip(t[0].windows(2)).flat_map(|(k, v)| {
                (0..parts).map(move |i| {
                    let f = i as f64 / parts as f64;
                    (k[0] + (k[1] - k[0]) * f, v[0] + (v[1] - v[0]) * f)
                })
            });
            let (at, kept): (Vec<f64>, Vec<f64>) =
                lines.chain([(k[k.len() - 1], t[0][k.len() - 1])]).unzip();
            Radial::new(
                radii(&at),
                kept.iter().map(|v| (8192. / v) as f32).collect(),
            )
        })
        .filter(|r| changes(r, 1e-4));
    // Red's and blue's scale relative to green, in 2^-23 from 2^14.
    let chromatic = group(counts[1], 2, 2)
        .and_then(|(k, t)| {
            let scale = |t: &[f64]| {
                t.iter()
                    .map(|v| (1. + (v - 16384.) / 8_388_608.) as f32)
                    .collect()
            };
            Some([
                Radial::new(radii(k), scale(t[0]))?,
                Radial::new(radii(k), scale(t[1]))?,
            ])
        })
        .filter(|[a, b]| changes(a, 1e-7) || changes(b, 1e-7));
    let distortion = group(counts[2], 5, 1)
        .and_then(|(k, t)| canon_distortion(k, t[0], unit, half, size_px))
        .filter(|r| changes(r, 1e-6));
    Some(LensCorrection {
        source: "Canon built-in".into(),
        // A "Camera Settings" profile in Adobe's DNGs, which Enable Profile Corrections
        // applies (Camera Raw 18.7 renders of an R6 Mark III photo).
        default_on: false,
        vignetting,
        distortion,
        chromatic,
        // Standing in for a profile: switched on, it corrects all three.
        always: None,
    })
}

// RAWmakase 37ea972: crates/rawmakase-model/src/lens/embedded.rs
/// Canon's distortion table, green's source radius over the output radius in 2^-14 at
/// each knot (`at`, in knots), as Adobe turns it into its profile: a least-squares fit of 1, r², r⁴
/// and r⁶ to the values past the centre, residuals weighted by the radius, the
/// knots' radii scaled by the values' mean to the power 0.75; of which the change
/// from 1 over the constant term, at 80%, is the profile's ratio, scaled so the
/// default crop's corner (the developed image's, `size_px`) stays in place. This
/// matches the profiles DNG Converter 18.7 writes for edited copies of an R6 Mark III
/// file (single knots raised or lowered by 1%, smooth tables, the real one scaled by
/// -1 to 1.5) to within 0.15 px.
fn canon_distortion(
    at: &[f64],
    values: &[f64],
    unit: f64,
    half: f64,
    size_px: [f64; 2],
) -> Option<Radial> {
    let d: Vec<f64> = values[1..]
        .iter()
        .map(|v| (v / 16384.).min(CANON_DISTORTION_MAX))
        .collect();
    if d.len() < 4 {
        return None;
    }
    let last = at[at.len() - 1];
    let spread = (d.iter().sum::<f64>() / d.len() as f64).powf(0.75);
    let mut normal = [[0.; 5]; 4];
    for (k, y) in at[1..].iter().zip(&d) {
        let t2 = (k / last * spread).powi(2);
        let row = [1., t2, t2 * t2, t2 * t2 * t2];
        for (j, n) in normal.iter_mut().enumerate() {
            for i in 0..4 {
                n[i] += t2 * row[j] * row[i];
            }
            n[4] += t2 * row[j] * y;
        }
    }
    let a = solve4(normal).filter(|a| a[0] > 0.)?;
    let c = [1, 2, 3].map(|i| 0.8 * a[i] / a[0]);
    let ratio = |px: f64| {
        let t2 = (px / (last * unit)).powi(2);
        1. + t2 * (c[0] + t2 * (c[1] + t2 * c[2]))
    };
    // Adobe's ScaleFactor keeps the corner about 2·10⁻⁵ further in (0.1 px).
    let scale = 1. / ratio(size_px[0].hypot(size_px[1]) * 0.5);
    // Radial's most knots: with 33, an RF135mm F1.8's curve, which bends sharply near
    // the corner, is 0.08 px off between them.
    let radii: Vec<f32> = (0..64).map(|i| i as f32 / 63.).collect();
    let values: Vec<f32> = (radii.iter())
        .map(|r| (scale * ratio(f64::from(*r) * half)) as f32)
        .collect();
    // The source radius grows with the output radius.
    let sources: Vec<f32> = radii.iter().zip(&values).map(|(r, v)| r * v).collect();
    (sources.windows(2).all(|p| p[0] < p[1])).then(|| Radial::new(radii, values))?
}

#[cfg(test)]
mod tests {
    use rawler::formats::bmff::ext_cr3::cr3desc::Cr3DescBox;

    use crate::rm_lens_builtin::test_files::{
        adobe, digest, largest_difference, read_framed, tiff,
    };
    use crate::rm_lens_builtin::{Amounts, LensCorrection};

    /// An ISO base media box.
    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [&(8 + body.len() as u32).to_be_bytes()[..], kind, body].concat()
    }

    /// A full box: version 0, no flags, then `body`.
    fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        bx(kind, &[&[0; 4][..], body].concat())
    }

    /// A CR3 of `model` whose timed metadata (the CTMD track's one sample) is
    /// `records`: RAWmakase's, with the boxes rawler's reader requires around it.
    fn cr3(model: &str, records: &[u8]) -> Vec<u8> {
        let ftyp = bx(b"ftyp", b"crx \0\0\0\x01crx isom");
        let name = [model.as_bytes(), b"\0"].concat();
        let cmt1 = tiff(&[vec![(0x0110, 2, name.len() as u32, name)]]);
        // rawler reads CMT2 to CMT4 as TIFFs too.
        let other = tiff(&[vec![(0x0001, 3, 1, vec![0; 2])]]);
        // Canon's metadata box in the `moov`, named by its UUID.
        let canon = [
            &Cr3DescBox::UUID[..],
            &bx(b"CNCV", &[0; 30]),
            &full(b"CCTP", &[0; 8]),
            &bx(b"CTBO", &[]),
            &bx(b"CMT1", &cmt1),
            &bx(b"CMT2", &other),
            &bx(b"CMT3", &other),
            &bx(b"CMT4", &other),
            &bx(b"THMB", &[]),
        ]
        .concat();
        let moov = |at: u64| {
            // Six reserved bytes, the data reference and a count of no records.
            let entry = bx(b"CTMD", &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
            let stsd = [&[0, 0, 0, 0, 0, 0, 0, 1][..], &entry].concat();
            let stsz = [
                [0; 4],
                (records.len() as u32).to_be_bytes(),
                1u32.to_be_bytes(),
            ]
            .concat();
            let co64 = [&[0, 0, 0, 0, 0, 0, 0, 1][..], &at.to_be_bytes()].concat();
            let stbl = [
                bx(b"stsd", &stsd),
                full(b"stts", &[0; 4]),
                full(b"stsc", &[0; 4]),
                bx(b"stsz", &stsz),
                bx(b"co64", &co64),
            ]
            .concat();
            let minf = [bx(b"dinf", &[]), bx(b"stbl", &stbl)].concat();
            let mdia = [
                full(b"mdhd", &[0; 18]),
                full(b"hdlr", &[]),
                bx(b"minf", &minf),
            ]
            .concat();
            let trak = bx(b"trak", &[full(b"tkhd", &[]), bx(b"mdia", &mdia)].concat());
            bx(
                b"moov",
                &[full(b"mvhd", &[0; 16]), bx(b"uuid", &canon), trak].concat(),
            )
        };
        let at = (ftyp.len() + moov(0).len() + 8) as u64;
        [ftyp, moov(at), bx(b"mdat", records)].concat()
    }

    /// The timed metadata's records: a time stamp, then record 14 holding `tables`.
    fn ctmd(tables: &[u16]) -> Vec<u8> {
        let record = |kind: u16, data: &[u8]| {
            let len = (12 + data.len()) as u32;
            [
                &len.to_le_bytes()[..],
                &kind.to_le_bytes(),
                &[0, 0, 0, 1, 0xff, 0xff],
                data,
            ]
            .concat()
        };
        let lens: Vec<u8> = tables.iter().flat_map(|v| v.to_le_bytes()).collect();
        [record(1, &[0; 12]), record(14, &lens)].concat()
    }

    /// Record 14 of IMG_4241.CR3 (raw.pixls.us, EOS R6 Mark III, RF50mm F1.4 L VCM at
    /// f/7.1): the counts, then seven arrays of 23 slots: vignetting and its knots, red,
    /// blue and their knots, distortion and its knots.
    const R6III: [u16; 164] = [
        17, 17, 17, 8191, 8079, 7767, 7550, 7302, 7028, 6734, 6428, 6275, 6124, 5975, 5889, 5834,
        5766, 5692, 5505, 5095, 0, 0, 0, 0, 0, 0, 0, 757, 1514, 1893, 2271, 2650, 3028, 3407, 3596,
        3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0, 0, 0, 0, 0, 16384, 16398, 16409,
        16416, 16426, 16441, 16465, 16502, 16531, 16566, 16612, 16646, 16671, 16704, 16738, 16773,
        16807, 0, 0, 0, 0, 0, 0, 16384, 16442, 16499, 16520, 16531, 16526, 16499, 16436, 16384,
        16316, 16226, 16158, 16108, 16038, 15959, 15881, 15803, 0, 0, 0, 0, 0, 0, 0, 757, 1514,
        1893, 2271, 2650, 3028, 3407, 3596, 3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0,
        0, 0, 0, 0, 16384, 16379, 16412, 16437, 16466, 16499, 16532, 16562, 16576, 16586, 16594,
        16596, 16596, 16595, 16593, 16588, 16582, 0, 0, 0, 0, 0, 0, 0, 757, 1514, 1893, 2271, 2650,
        3028, 3407, 3596, 3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0, 0, 0, 0, 0,
    ];

    /// The default crop of IMG_4241.CR3.
    const R6III_SIZE: [f64; 2] = [6960., 4640.];

    /// Record 14 of RAW.CR3 (raw.pixls.us, EOS R5 Mark II, RF135mm F1.8 L IS USM at f/1.8).
    const R5II: [u16; 164] = [
        17, 17, 17, 8191, 8138, 7334, 6790, 6224, 5644, 5057, 4477, 4191, 3910, 3632, 3446, 3307,
        3109, 2796, 2388, 1978, 0, 0, 0, 0, 0, 0, 0, 757, 1514, 1893, 2271, 2650, 3028, 3407, 3596,
        3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0, 0, 0, 0, 0, 16384, 16277, 16174,
        16121, 16065, 16010, 15957, 15911, 15894, 15884, 15885, 15894, 15906, 15916, 15927, 15939,
        15950, 0, 0, 0, 0, 0, 0, 16384, 16487, 16574, 16615, 16660, 16707, 16754, 16795, 16811,
        16821, 16814, 16798, 16778, 16759, 16739, 16720, 16700, 0, 0, 0, 0, 0, 0, 0, 757, 1514,
        1893, 2271, 2650, 3028, 3407, 3596, 3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0,
        0, 0, 0, 0, 16384, 16344, 16409, 16456, 16511, 16573, 16639, 16703, 16732, 16759, 16780,
        16789, 16795, 16798, 16800, 16799, 16797, 0, 0, 0, 0, 0, 0, 0, 757, 1514, 1893, 2271, 2650,
        3028, 3407, 3596, 3786, 3975, 4088, 4164, 4259, 4353, 4448, 4543, 0, 0, 0, 0, 0, 0,
    ];

    fn canon_file(model: &str, tables: &[u16]) -> Vec<u8> {
        cr3(model, &ctmd(tables))
    }

    /// What Adobe DNG Converter 18.7 writes into a DNG of a Canon file: the "Camera
    /// Settings" profile (focal length, sensor format factor, ScaleFactor, radial
    /// distortion, piecewise vignetting) and the red and blue planes of its warp
    /// (green's is the identity), whose radius 1 is the farthest corner of the DNG's
    /// active area from its centre, which is the default crop's.
    struct CanonAdobe {
        focal: f64,
        format_factor: f64,
        scale: f64,
        k: [f64; 3],
        piecewise: &'static [(f64, f64)],
        warp: [[f64; 4]; 2],
        corner: [f64; 2],
    }

    /// The profile's source radius in pixels at output radius `px`, for a default crop
    /// of `crop` pixels: its radius is in focal lengths, the crop's diagonal being
    /// 43.2666 mm over the sensor format factor.
    fn canon_profile(a: &CanonAdobe, crop: [f64; 2], px: f64) -> f64 {
        let diagonal = crop[0].hypot(crop[1]);
        let rho = px / diagonal * 43.2666 / a.format_factor / a.focal;
        let r2 = rho * rho;
        px * a.scale * (1. + r2 * (a.k[0] + r2 * (a.k[1] + r2 * a.k[2])))
    }

    /// `c` against Adobe's profile and warp, over a developed image (the default
    /// crop) of `size`.
    fn check_canon(c: &LensCorrection, size: [f64; 2], a: &CanonAdobe, within: f64) {
        assert!(!c.default_on);
        assert_eq!(c.source, "Canon built-in");
        assert_eq!(c.always, None, "switched on, it corrects all three");
        let unit = a.corner[0].hypot(a.corner[1]);
        let plane = |k: [f64; 4], px: f64| adobe(k, unit, px) / px;
        let off = largest_difference(c, size, &|ch, px| {
            let green = canon_profile(a, size, px);
            match ch {
                0 => green * plane(a.warp[0], green),
                2 => green * plane(a.warp[1], green),
                _ => green,
            }
        });
        assert!(off < within, "{off} px from Adobe's profile");
        let half = size[0].hypot(size[1]) / 2.;
        let diagonal = size[0].hypot(size[1]);
        for &(rho, kept) in a.piecewise {
            let px = rho * a.focal * a.format_factor / 43.2666 * diagonal;
            let ours = 1. / f64::from(c.vignetting_gain((px / half) as f32, 1.));
            assert!((ours - kept).abs() < 0.001, "{rho}: {ours} against {kept}");
        }
    }

    #[test]
    fn reads_canon_r6_mark_iii_corrections_as_adobe_does() {
        let c = read_framed(&canon_file("Canon EOS R6 Mark III", &R6III), R6III_SIZE).unwrap();
        let adobe = CanonAdobe {
            focal: 50.,
            format_factor: 1.,
            scale: 0.989001,
            k: [0.083019, 0.016376, -0.764324],
            piecewise: &[
                (0.080435, 0.986326),
                (0.16087, 0.948236),
                (0.241305, 0.891466),
                (0.32174, 0.822122),
                (0.402282, 0.74765),
                (0.442446, 0.712245),
                (0.482717, 0.622024),
            ],
            warp: [
                [
                    1.0000037075930996,
                    -4.231126716690595e-06,
                    2.083027506703311e-05,
                    1.2193982455706968e-05,
                ],
                [
                    1.0000062828295775,
                    6.320382627848142e-05,
                    -7.571489431295625e-05,
                    -2.270913473936408e-05,
                ],
            ],
            corner: [3492., 2332.],
        };
        check_canon(&c, R6III_SIZE, &adobe, 0.15);
        // Without the correction, halfway to the corner is 15 px out.
        assert!(canon_profile(&adobe, R6III_SIZE, 2091.) < 2091. - 14.);
    }

    #[test]
    fn reads_canon_r5_mark_ii_corrections_as_adobe_does() {
        // The default crop of the 8222 × 5488 decoded image.
        let size = [8192., 5464.];
        let c = read_framed(&canon_file("Canon EOS R5m2", &R5II), size).unwrap();
        let adobe = CanonAdobe {
            focal: 135.,
            format_factor: 1.001926,
            scale: 0.97803,
            k: [1.135087, 0.747337, -422.990386],
            piecewise: &[
                (0.029741, 0.993529),
                (0.059481, 0.895373),
                (0.089222, 0.759858),
                (0.118962, 0.617385),
                (0.148742, 0.477353),
                (0.163593, 0.403736),
                (0.178483, 0.241485),
            ],
            warp: [
                [
                    0.9999888454181883,
                    -0.00010766927304507836,
                    5.443760114059718e-05,
                    4.93985093601397e-06,
                ],
                [
                    1.0000122089263515,
                    7.45386255399043e-05,
                    -5.82086411194138e-06,
                    -3.114694366284039e-05,
                ],
            ],
            corner: [4112., 2746.],
        };
        // 0.16 px at the corner, where Adobe's ScaleFactor frames 2·10⁻⁵ tighter.
        check_canon(&c, size, &adobe, 0.2);
    }

    /// Adobe DNG Converter's profiles of copies of IMG_4241.CR3 with edited distortion
    /// tables: values above 1.0275 count as 1.0275, and a table's change from 1 is
    /// fitted at 80% of its size.
    #[test]
    fn fits_canon_distortion_as_adobe_does() {
        // The distortion's values and knots are the sixth and seventh arrays.
        let (values, knots) = (3 + 5 * 23, 3 + 6 * 23);
        let knots: [u16; 17] = R6III[knots..knots + 17].try_into().unwrap();
        for (name, table, k) in [
            (
                "a quartic",
                knots.map(|x| (16384. * (1. + 0.01 * (f64::from(x) / 4543.).powi(4))).round()),
                [-0.000256, 0.147874, 0.000152],
            ),
            (
                "a spike past the limit",
                std::array::from_fn(|i| if i == 6 { 18024. } else { 16384. }),
                [0.531223, -4.283277, 9.71879],
            ),
        ] {
            let mut tables = R6III;
            for (slot, v) in tables[values..values + 17].iter_mut().zip(table) {
                *slot = v as u16;
            }
            let file = canon_file("Canon EOS R6 Mark III", &tables);
            let c = read_framed(&file, R6III_SIZE).unwrap();
            let scale = |px: f64| {
                let rho = px / 6960f64.hypot(4640.) * 43.2666 / 50.;
                let r2 = rho * rho;
                1. + r2 * (k[0] + r2 * (k[1] + r2 * k[2]))
            };
            // Both framed to keep the default crop's corner.
            let corner = R6III_SIZE[0].hypot(R6III_SIZE[1]) / 2.;
            let half = corner;
            let ours =
                |px: f64| f64::from(c.radial_scale_with((px / half) as f32, Amounts::FULL)[1]);
            let off = (0..=100)
                .map(|i| corner * f64::from(i) / 100.)
                .map(|px| px * (ours(px) / ours(corner) - scale(px) / scale(corner)).abs())
                .fold(0., f64::max);
            assert!(off < 0.3, "{name}: {off} px from Adobe's profile");
        }
    }

    /// Adobe applies no camera data for other bodies, although the R50 V stores the
    /// same record.
    #[test]
    fn canon_tables_of_other_bodies_are_not_read() {
        let file = canon_file("Canon EOS R50 V", &R6III);
        assert!(read_framed(&file, R6III_SIZE).is_none());
    }

    #[test]
    fn malformed_canon_tables_are_not_read() {
        let with = |i: usize, v: u16| {
            let mut t = R6III;
            t[i] = v;
            t
        };
        let r6 = |t: &[u16]| read_framed(&canon_file("Canon EOS R6 Mark III", t), R6III_SIZE);
        for (name, t) in [
            ("count past the slots", with(0, 24)),
            ("single knot", with(2, 1)),
            ("knots out of order", with(3 + 23 + 5, 9000)),
        ] {
            let c = r6(&t).unwrap_or_default();
            let part = match name {
                "single knot" => c.distortion.is_none(),
                _ => c.vignetting.is_none(),
            };
            assert!(part, "{name}");
        }
        // Counts past the slots, of each group: the other groups are still read.
        for (i, count) in [0, 1, 2].into_iter().flat_map(|i| [(i, 24), (i, u16::MAX)]) {
            let c = r6(&with(i, count)).unwrap();
            let parts = [
                c.vignetting.is_none(),
                c.chromatic.is_none(),
                c.distortion.is_none(),
            ];
            assert_eq!(
                parts,
                std::array::from_fn(|j| j == i),
                "count {i} = {count}"
            );
        }
        assert!(r6(&R6III[..100]).is_none());
    }

    /// Today's reader, pinned as `todays_readers_are_pinned` pins the others: a
    /// change meant to move it updates the number; any other change fails.
    #[test]
    fn todays_reader_is_pinned() {
        let file = canon_file("Canon EOS R6 Mark III", &R6III);
        assert_eq!(
            digest(&read_framed(&file, R6III_SIZE)),
            16_636_585_091_394_672_644
        );
    }
}
