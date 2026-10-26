//! The small files the readers' tests build, shared by each make's tests
//! and the pin in `rm_lens_builtin.rs`, the digest that pin compares, and
//! the measures the makers' tests take against Adobe's numbers.
//!
//! `directory`, `hex`, `adobe` and `largest_difference` are RAWmakase's
//! (MIT) test helpers @ af35e25 (`crates/rawmakase-model/src/lens/embedded.rs`),
//! measured out to the developed image's corner where RAWmakase measures to
//! the default crop's: here they are the same image.

use super::fuji::{FUJI_CHROMATIC, FUJI_DISTORTION, FUJI_IFD, FUJI_VIGNETTING, RAF_TIFF_POINTER};
use super::nikon::NEF_INFO;
use super::sony::{SONY_CHROMATIC, SONY_DISTORTION, SONY_VIGNETTING};
use super::{Amounts, LensCorrection, SUB_IFDS};

/// (tag, type, count, data)
pub(super) type Entry = (u16, u16, u32, Vec<u8>);

/// Little-endian TIFF with the given directories, each pointed to by the
/// previous one's first entry. RAWmakase's builder, as written.
pub(super) fn tiff(dirs: &[Vec<Entry>]) -> Vec<u8> {
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
pub(super) fn raf(entries: Vec<Entry>) -> Vec<u8> {
    let fuji = tiff(&[vec![(FUJI_IFD, 13, 1, vec![0; 4])], entries]);
    let mut raf = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
    raf.resize(128, 0);
    raf[RAF_TIFF_POINTER..RAF_TIFF_POINTER + 4].copy_from_slice(&128u32.to_be_bytes());
    raf.extend(fuji);
    raf
}

/// A RAF with vignetting, CA on red and an identity distortion.
pub(super) fn fuji_file() -> Vec<u8> {
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
    raf(vec![
        (FUJI_DISTORTION, 10, 23, rationals(&dist)),
        (FUJI_CHROMATIC, 10, 34, rationals(&ca)),
        (FUJI_VIGNETTING, 10, 23, rationals(&vig)),
    ])
}

/// An opcode list holding `ops` (id, parameters), big-endian.
fn opcode_list(ops: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = (ops.len() as u32).to_be_bytes().to_vec();
    for (id, params) in ops {
        for v in [*id, 0x0103_0000, 0, params.len() as u32] {
            b.extend(v.to_be_bytes());
        }
        b.extend(params);
    }
    b
}

pub(super) fn be(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_be_bytes()).collect()
}

/// A DNG as DJI writes it: vignetting and warp in OpcodeList3, a gain map
/// after them.
pub(super) fn dji_file() -> Vec<u8> {
    let vig = be(&[0.2599, 0.3424, 0.0630, 0.0346, -0.0300, 0.5, 0.5]);
    let mut w = 1u32.to_be_bytes().to_vec();
    w.extend(be(&[1., -0.06, 0., 0., 0., 0., 0.5, 0.5]));
    let list3 = opcode_list(&[(3, vig), (1, w), (9, vec![0; 8])]);
    tiff(&[
        vec![
            (SUB_IFDS, 4, 1, vec![0; 4]),
            (50706, 1, 4, vec![1, 4, 0, 0]),
        ],
        vec![
            (254, 4, 1, vec![0; 4]),
            (51022, 7, list3.len() as u32, list3),
        ],
    ])
}

pub(super) fn shorts(v: &[i16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A ZV-1's ARW-like TIFF whose first SubIFD holds Sony's three tables.
pub(super) fn sony_file(vig: &[i16], ca: &[i16], dist: &[i16]) -> Vec<u8> {
    sony_model_file("ZV-1", vig, ca, dist)
}

/// As `sony_file`, for the camera `model` (IFD0's Model).
pub(super) fn sony_model_file(model: &str, vig: &[i16], ca: &[i16], dist: &[i16]) -> Vec<u8> {
    let entry = |tag, v: &[i16]| (tag, 8, v.len() as u32, shorts(v));
    let model = [model.as_bytes(), b"\0"].concat();
    tiff(&[
        vec![
            (SUB_IFDS, 4, 1, vec![0; 4]),
            (0x0110, 2, model.len() as u32, model),
        ],
        vec![
            entry(SONY_VIGNETTING, vig),
            entry(SONY_CHROMATIC, ca),
            entry(SONY_DISTORTION, dist),
        ],
    ])
}

/// A ZV-1 stores fewer knots than its fixed 16 (CA: 32) slots, padded
/// with zeros (RAWmakase #403).
pub(super) const ZV1_VIG: [i16; 17] = [
    16, 0, 0, 0, 15, 63, 193, 413, 715, 1107, 1576, 2126, 2749, 3443, 4523, 5509, 6639,
];
pub(super) const ZV1_CA: [i16; 33] = [
    22, 0, 256, 384, 128, 128, 256, 384, 640, 896, 1152, 1408, 0, 1280, 1664, 1664, 1280, 896, 512,
    256, 128, 0, 256, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
pub(super) const ZV1_DIST: [i16; 17] = [
    11, 1265, 1218, 1083, 860, 562, 209, -179, -580, -972, -1353, -1725, 0, 0, 0, 0, 0,
];

/// The registry's `read` for a test file, which rawler cannot open: its
/// developed image is `size_px`.
pub(super) fn read_framed(file: &[u8], size_px: [f64; 2]) -> Option<LensCorrection> {
    super::read_sized(file, &|| Some(size_px))
}

/// Little-endian directory of `entries` (tag, type, count, data) at `at`, as a
/// file's or maker note's offsets count; the data follows it.
pub(super) fn directory(at: usize, entries: &[Entry]) -> Vec<u8> {
    let mut data_at = at + 2 + entries.len() * 12 + 4;
    let (mut out, mut data): (Vec<u8>, Vec<u8>) =
        ((entries.len() as u16).to_le_bytes().to_vec(), vec![]);
    for (tag, kind, count, bytes) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        if bytes.len() <= 4 {
            out.extend(bytes.iter().chain(&[0; 4]).take(4));
        } else {
            out.extend((data_at as u32).to_le_bytes());
            data.extend(bytes);
            data_at += bytes.len();
        }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(data);
    out
}

/// Bytes written as hex, as RAWmakase's tests copy a file's records.
pub(super) fn hex(s: &str) -> Vec<u8> {
    (s.split_whitespace())
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

/// Source radius in pixels at output radius `px` from Adobe's WarpRectilinear
/// coefficients, whose radius 1 is `unit` pixels.
pub(super) fn adobe(k: [f64; 4], unit: f64, px: f64) -> f64 {
    let r2 = (px / unit).powi(2);
    px * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * k[3])))
}

/// Largest difference in pixels, out to the corner of a developed image of
/// `size_px`, between the source radii of `c` and Adobe's warp.
pub(super) fn largest_difference(
    c: &LensCorrection,
    size_px: [f64; 2],
    adobe: &dyn Fn(usize, f64) -> f64,
) -> f64 {
    let half = size_px[0].hypot(size_px[1]) / 2.;
    (0..=200)
        .flat_map(|i| {
            let px = half * f64::from(i) / 200.;
            let scale = c.radial_scale_with((px / half) as f32, Amounts::FULL);
            (0..3).map(move |ch| (px * f64::from(scale[ch]) - adobe(ch, px)).abs())
        })
        .fold(0., f64::max)
}

/// Largest difference in pixels, out to the corner of a developed image of
/// `size_px`, between red's and blue's displacement from green in `c` and in
/// Adobe's warp, and Adobe's largest displacement. RAWmakase's @ d04ef4f.
pub(super) fn largest_ca_difference(
    c: &LensCorrection,
    size_px: [f64; 2],
    adobe: &dyn Fn(usize, f64) -> f64,
) -> (f64, f64) {
    let half = size_px[0].hypot(size_px[1]) / 2.;
    (0..=200)
        .flat_map(|i| {
            let px = half * f64::from(i) / 200.;
            let scale = c.radial_scale_with((px / half) as f32, Amounts::FULL);
            [0, 2].map(|ch| {
                let ours = px * f64::from(scale[ch] - scale[1]);
                let theirs = adobe(ch, px) - adobe(1, px);
                ((ours - theirs).abs(), theirs.abs())
            })
        })
        .fold((0., 0.), |(a, b), (x, y)| (a.max(x), b.max(y)))
}

/// A NEF of the camera `model` whose raw SubIFD's NEFInfo holds `records`
/// (tag, bytes), RAWmakase's builder, with what rawler needs to size it
/// without a pixel (`developed_size_px`): IFD0's Make and Model and a maker
/// note with a white balance, and a raw image of `size` at 14 bits,
/// uncompressed, which no pixel of the file holds.
pub(super) fn nef(model: &str, size: [u32; 2], records: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<Entry> = (records.iter())
        .map(|(tag, r)| (*tag, 7, r.len() as u32, r.clone()))
        .collect();
    // A TIFF after "Nikon\0" and four version bytes.
    let info = [&b"Nikon\0\x01\x01\0\0"[..], &tiff(&[entries])].concat();
    // Nikon's maker note, a TIFF after "Nikon\0" and four bytes, holding
    // NefWB0 (0x000C).
    let note = [
        &b"Nikon\0\x02\x10\0\0"[..],
        &tiff(&[vec![(0x000c, 3, 2, vec![1, 0, 1, 0])]]),
    ]
    .concat();
    let model = [model.as_bytes(), b"\0"].concat();
    let long = |v: u32| v.to_le_bytes().to_vec();
    tiff(&[
        vec![
            (SUB_IFDS, 4, 1, vec![0; 4]),
            (0x010f, 2, 18, b"NIKON CORPORATION\0".to_vec()),
            (0x0110, 2, model.len() as u32, model),
            (0x927c, 7, note.len() as u32, note),
        ],
        vec![
            // NewSubFileType 0: the raw image.
            (0x00fe, 4, 1, long(0)),
            (0x0100, 4, 1, long(size[0])),
            (0x0101, 4, 1, long(size[1])),
            (0x0102, 3, 1, vec![14, 0]),
            (0x0103, 3, 1, vec![1, 0]),
            (0x0111, 4, 1, long(0)),
            (0x0116, 4, 1, long(size[1])),
            (0x0117, 4, 1, long(1)),
            (NEF_INFO, 7, info.len() as u32, info),
        ],
    ])
}

/// A Z body's NEF whose NEFInfo holds lens-correction records 5 to 8.
pub(super) fn nikon_file(model: &str, size: [u32; 2], records: [&str; 4]) -> Vec<u8> {
    let records: Vec<(u16, Vec<u8>)> = (5..).zip(records).map(|(tag, r)| (tag, hex(r))).collect();
    nef(model, size, &records)
}

/// A P1100's NRW whose NEFInfo holds the Coolpix tables, records 2 and 3.
pub(super) fn coolpix_file(records: [&str; 2]) -> Vec<u8> {
    let records: Vec<(u16, Vec<u8>)> = (2..).zip(records).map(|(tag, r)| (tag, hex(r))).collect();
    nef("COOLPIX P1100", [4624, 3470], &records)
}

/// DSCN0173.NRW (raw.pixls.us, COOLPIX P1100): NEFInfo records 2 and 3.
/// RAWmakase's records, as written.
pub(super) const P1100: [&str; 2] = [
    "30 31 30 30 03 03 11 01 00 00 00 00 a0 86 01 00 96 03 00 00 a0 86 01 00 58 0e 00 00
     a0 86 01 00 4c 20 00 00 a0 86 01 00 77 39 00 00 a0 86 01 00 e3 59 00 00 a0 86 01 00
     9d 81 00 00 a0 86 01 00 b2 b0 00 00 a0 86 01 00 33 e7 00 00 a0 86 01 00 33 25 01 00
     a0 86 01 00 c7 6a 01 00 a0 86 01 00 06 b8 01 00 a0 86 01 00 06 0d 02 00 a0 86 01 00
     e1 69 02 00 a0 86 01 00 b0 ce 02 00 a0 86 01 00 8d 3b 03 00 a0 86 01 00 90 b0 03 00
     a0 86 01 00 00 00 07 91",
    "30 31 30 30 03 22 01 00 00 00 00 00 64 00 00 00 11 00 00 00 64 00 00 00 23 00 00 00
     64 00 00 00 34 00 00 00 64 00 00 00 45 00 00 00 64 00 00 00 56 00 00 00 64 00 00 00
     67 00 00 00 64 00 00 00 78 00 00 00 64 00 00 00 89 00 00 00 64 00 00 00 9a 00 00 00
     64 00 00 00 aa 00 00 00 64 00 00 00 bb 00 00 00 64 00 00 00 cc 00 00 00 64 00 00 00
     dd 00 00 00 64 00 00 00 ed 00 00 00 64 00 00 00 ff 00 00 00 64 00 00 00 10 01 00 00
     64 00 00 00 00 00 00 00 64 00 00 00 08 00 00 00 64 00 00 00 11 00 00 00 64 00 00 00
     19 00 00 00 64 00 00 00 22 00 00 00 64 00 00 00 2b 00 00 00 64 00 00 00 34 00 00 00
     64 00 00 00 3d 00 00 00 64 00 00 00 46 00 00 00 64 00 00 00 50 00 00 00 64 00 00 00
     59 00 00 00 64 00 00 00 63 00 00 00 64 00 00 00 6d 00 00 00 64 00 00 00 77 00 00 00
     64 00 00 00 80 00 00 00 64 00 00 00 8a 00 00 00 64 00 00 00 93 00 00 00 64 00 00 00
     00 00 aa 73",
];

/// 1-Nikon-Z7-RAW-14bit-uncompressed-L.NEF (raw.pixls.us, Z 7, NIKKOR Z
/// 24-70mm f/4 S at 56mm f/5.6): a lens that needs the correction (mode 3).
/// RAWmakase's records, as written.
pub(super) const Z7: [&str; 4] = [
    "30 31 30 30 03 01 00 00 9a 59 01 00 00 10 00 00 04 00 00 00 31 42 00 00 00 00 10 00
     9f d9 ff ff 00 00 10 00 de a1 00 00 00 00 10 00 00 00 00 00 01 00 00 00 00 00 00 00
     01 00 00 00 00 00 00 00 92 fd 00 00 00 00 10 00 00 00 00 00 00 00 00 00 00 00 ec 5b",
    "30 31 30 30 01 01 00 00 9a 59 01 00 00 10 00 00 08 00 00 00 6a 20 ee ff 00 00 10 00
     00 00 00 00 01 00 00 00 5e 6e 21 00 00 00 10 00 00 00 00 00 01 00 00 00 86 6b f3 ff
     00 00 10 00 00 00 00 00 01 00 00 00 80 ca 07 00 00 00 10 00 00 00 00 00 01 00 00 00
     01 00 00 00 01 00 00 00 00 00 00 00 1d 02 00 00 00 04 00 00 00 00 00 00 00 00 00 00
     00 00 d8 79",
    "30 31 30 30 01 02 00 00 9a 59 01 00 00 10 00 00 03 00 00 00 14 00 00 00 00 00 10 00
     c0 fe ff ff 00 00 10 00 00 00 00 00 01 00 00 00 99 ff ff ff 00 00 10 00 03 00 00 00
     af ff ff ff 00 00 10 00 66 01 00 00 00 00 10 00 00 00 00 00 01 00 00 00 ab 00 00 00
     00 00 10 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 2c 47",
    "30 31 30 30 36 ff ff ff 00 01 00 00 f7 01 00 00 00 01 00 00 00 00 ec e2",
];

/// Nikon_Z8_high_efficiency_low.NEF (raw.pixls.us, Z 8, NIKKOR Z 14-24mm
/// f/2.8 S at 24mm f/8): a lens whose correction is optional (mode 1).
/// RAWmakase's records, as written.
pub(super) const Z8: [&str; 4] = [
    "30 31 30 30 01 01 00 00 9a 59 01 00 00 10 00 00 04 00 00 00 bc 24 00 00 00 00 10 00
     3a d5 ff ff 00 00 10 00 7d 54 00 00 00 00 10 00 00 00 00 00 01 00 00 00 00 00 00 00
     01 00 00 00 00 00 00 00 a7 50 00 00 00 00 10 00 00 00 00 00 00 00 00 00 00 00 fe fb",
    "30 31 30 30 01 01 00 00 9a 59 01 00 00 10 00 00 08 00 00 00 00 00 00 00 00 00 10 00
     00 00 00 00 01 00 00 00 24 26 01 00 00 00 10 00 00 00 00 00 01 00 00 00 74 33 00 00
     00 00 10 00 00 00 00 00 01 00 00 00 cf 33 08 00 00 00 10 00 00 00 00 00 01 00 00 00
     01 00 00 00 01 00 00 00 00 00 00 00 9a 01 00 00 00 04 00 00 00 00 00 00 00 00 00 00
     00 00 5e 0a",
    "30 31 30 30 01 02 00 00 9a 59 01 00 00 10 00 00 03 00 00 00 5a fd ff ff 00 00 10 00
     fa 03 00 00 00 00 10 00 00 00 00 00 01 00 00 00 2e ff ff ff 00 00 10 00 03 00 00 00
     f6 02 00 00 00 00 10 00 c1 fb ff ff 00 00 10 00 00 00 00 00 01 00 00 00 8c 01 00 00
     00 00 10 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 62 d6",
    "30 31 30 30 1c f9 ff ff 00 01 00 00 a7 04 00 00 00 01 00 00 00 00 13 e5",
];

/// What `c` serializes to, every number to seven significant digits, as
/// one FNV-1a hash: exact enough to catch any change to a reader, loose
/// enough to ignore a platform's last-bit difference in `powf`.
pub(super) fn digest(c: &Option<LensCorrection>) -> u64 {
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Number(n) => out.push_str(&format!("{:.6e}", n.as_f64().unwrap())),
            serde_json::Value::Array(a) => {
                out.push('[');
                for x in a {
                    walk(x, out);
                    out.push(',');
                }
                out.push(']');
            }
            serde_json::Value::Object(o) => {
                out.push('{');
                for (k, x) in o {
                    out.push_str(k);
                    out.push(':');
                    walk(x, out);
                    out.push(',');
                }
                out.push('}');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut text = String::new();
    walk(&serde_json::to_value(c).unwrap(), &mut text);
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
