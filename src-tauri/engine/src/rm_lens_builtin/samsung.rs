//! Samsung's built-in lens correction, read from an SRW.
//!
//! Samsung NX bodies (NX200 and later, NX mini, Galaxy NX) and the EX2F
//! store the camera's distortion and lateral CA models in the maker note:
//! Distortion (0xA050, eight signed longs) and ChromaticAberration (0xA051,
//! 22 shorts), each value offset by one of the eleven of its EncryptionKey
//! (0xA020). Distortion: four mantissas, each followed by a power of ten, of
//! green's source radius over the output radius r in pixels,
//! c0 + c1 r² + c2 r⁴ + c3 r⁶. CA: a count of 7, the radii in pixels, then
//! red's and blue's scale relative to green at each, in 1/10⁴, of which
//! Adobe applies half (on the EX2F three quarters of red's and a quarter of
//! blue's).
//!
//! Adobe writes them into its DNGs as a warp that always applies, so the
//! correction starts on and the parts the file stores are always the
//! camera's. Adobe ignores the data for the lenses and compacts it has its
//! own profiles for (LensType 1-3, EX1, WB2000) and on the first NX bodies
//! (NX5, NX10, NX100, NX11), and so does this: a lens profile corrects those.
//! No vignetting: the bodies read store none.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase @ 0b7d217 (MIT):
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Samsung reader, the
//!   lenses and bodies it leaves to Adobe's profiles, and its tests.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - IFD0 and the maker note are read with rawler's TIFF reader in place of
//!   `rawmakase-tiff`, from the file's bytes rather than its path: the note
//!   as rawler's own SRW decoder reads it (`IFD::parse_makernote`, offsets
//!   relative to the note), its values typed by rawler.
//! - Radius 1 is the developed image's half diagonal, as in every reader
//!   here: rawler crops an SRW to its default crop, so the correction is
//!   centred on the default crop, as Adobe's is, where RAWmakase centres it
//!   on the decoded image (1 px apart on the NX200). The model's radius is in
//!   pixels, so the correction in pixels is the same.
//! - The registry asks Samsung before Sony, whose reader answers for any
//!   TIFF, as RAWmakase chooses by Make before falling back to Sony.
//! - `always` names the parts the file stores.

use std::io::Cursor;

use rawler::formats::tiff::ifd::OffsetMode;
use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{GenericTiffReader, Value};
use rawler::tags::{ExifTag, TiffCommonTag};

use super::LensCorrection;
use super::fit::{make, maker_model};

/// The maker note's LensType, EncryptionKey, Distortion and ChromaticAberration.
pub(super) const SAMSUNG_LENS_TYPE: u16 = 0xa003;
pub(super) const SAMSUNG_KEY: u16 = 0xa020;
pub(super) const SAMSUNG_DISTORTION: u16 = 0xa050;
pub(super) const SAMSUNG_CHROMATIC: u16 = 0xa051;

// RAWmakase 0b7d217: crates/rawmakase-model/src/lens/embedded.rs
/// Lenses Adobe has its own profiles for (LensType 1–3: NX 30mm F2, 18-55mm and
/// 50-200mm), compacts it has profiles for, and the first NX bodies, whose data
/// Adobe does not use at all.
const SAMSUNG_UNUSED_LENSES: [u32; 3] = [1, 2, 3];
const SAMSUNG_UNUSED_MODELS: [&str; 6] = ["EX1", "WB2000", "NX5", "NX10", "NX100", "NX11"];

/// The models in an SRW's maker note; `None` for a file that is not a
/// Samsung TIFF, one whose data Adobe does not use, or one that stores
/// neither. `size_px` is the developed image's.
pub(super) fn read(file: &[u8], size_px: &dyn Fn() -> Option<[f64; 2]>) -> Option<LensCorrection> {
    if !super::is_tiff(file) {
        return None;
    }
    let tiff = GenericTiffReader::new_with_buffer(file, 0, 0, Some(0)).ok()?;
    let ifd0 = tiff.root_ifd();
    if !make(ifd0).starts_with("samsung") {
        return None;
    }
    // RAWmakase 0b7d217: crates/rawmakase-model/src/lens/embedded.rs
    let model = ifd0
        .get_entry(TiffCommonTag::Model)?
        .value
        .as_string()?
        .trim_end_matches(['\0', ' '])
        .to_owned();
    if SAMSUNG_UNUSED_MODELS.contains(&model.as_str()) {
        return None;
    }
    let exif = tiff.find_first_ifd_with_tag(ExifTag::MakerNotes)?;
    // At least one entry, which also keeps rawler's checks for other makers'
    // headers inside the note.
    let Value::Undefined(bytes) = &exif.get_entry(ExifTag::MakerNotes)?.value else {
        return None;
    };
    if bytes.len() < 18 {
        return None;
    }
    // A directory at the note's start, offsets counting from there.
    let note = exif
        .parse_makernote(&mut Cursor::new(file), OffsetMode::RelativeToIFD, &[])
        .ok()??;
    let lens = note
        .get_entry(SAMSUNG_LENS_TYPE)
        .and_then(|e| e.value.get_f32(0).ok().flatten());
    if lens.is_some_and(|l| SAMSUNG_UNUSED_LENSES.contains(&(l as u32))) {
        return None;
    }
    // Each value is offset by one of the EncryptionKey's eleven (0xA020).
    let key = match &note.get_entry(SAMSUNG_KEY)?.value {
        Value::Long(key) if key.len() == 11 => key,
        _ => return None,
    };
    let distortion = note
        .get_entry(SAMSUNG_DISTORTION)
        .and_then(|e| match &e.value {
            Value::SLong(d) if d.len() == 8 => Some(d),
            _ => None,
        })
        .map(|d| -> Vec<i32> {
            (d.iter().enumerate())
                .map(|(i, v)| (*v as u32).wrapping_add(key[i % 11]) as i32)
                .collect()
        });
    // After the count, which is not offset.
    let ca = note
        .get_entry(SAMSUNG_CHROMATIC)
        .and_then(|e| match &e.value {
            Value::Short(v) if v.len() == 22 => Some(v),
            _ => None,
        })
        .map(|v| -> Vec<f64> {
            (v.iter().enumerate())
                .map(|(i, c)| {
                    let k = if i == 0 { 0 } else { key[(i - 1) % 11] };
                    f64::from(c.wrapping_sub(k as u16))
                })
                .collect()
        })
        .filter(|v| v[0] == 7.);
    // Green's source radius over the output radius r, in pixels: c0 + c1 r² + c2 r⁴ +
    // c3 r⁶, each c a mantissa and a power of ten. A zero c0 is no model.
    let c: Option<[f64; 4]> = distortion
        .map(|d| std::array::from_fn(|j| f64::from(d[2 * j]) * 10f64.powi(d[2 * j + 1])))
        .filter(|c| c[0] > 0. && c.iter().all(|x| x.is_finite()));
    let green = |r: f64| {
        let Some(c) = c else { return Some(r) };
        let r2 = r * r;
        Some(r * (c[0] + r2 * (c[1] + r2 * (c[2] + r2 * c[3]))))
    };
    // Adobe uses the first six radii and values, which hold one value per channel on
    // every file surveyed (blue's seventh often differs). It applies half of each
    // change, and on the EX2F three quarters of red's and a quarter of blue's.
    let ca = ca.filter(|v| v[1] > 0. && v[1..=6].is_sorted_by(|a, b| a < b));
    let share = if model == "EX2F" {
        [0.75, 0.25]
    } else {
        [0.5, 0.5]
    };
    let scale = |at: usize, g: f64| {
        let Some(v) = &ca else { return 1. };
        let (knots, values) = (&v[1..7], &v[at..at + 6]);
        let i = knots.iter().position(|k| g < *k).unwrap_or(6);
        let value = match i {
            0 => values[0],
            6 => values[5],
            i => {
                let t = (g - knots[i - 1]) / (knots[i] - knots[i - 1]);
                values[i - 1] + t * (values[i] - values[i - 1])
            }
        };
        1. + share[usize::from(at == 15)] * (value / 10_000. - 1.)
    };
    let (red, blue) = (|g| scale(8, g), |g| scale(15, g));
    let chromatic = ca.is_some().then_some([&red as &dyn Fn(f64) -> f64, &blue]);
    let (distortion, chromatic) = maker_model(size_px()?, 1., &green, chromatic);
    let mut c = LensCorrection {
        source: "Samsung built-in".into(),
        // Adobe writes it into its DNGs as their warp, which always applies.
        default_on: true,
        vignetting: None,
        distortion,
        chromatic,
        always: None,
        centre: None,
    };
    c.always = Some(c.held_parts());
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::{SAMSUNG_CHROMATIC, SAMSUNG_DISTORTION, SAMSUNG_KEY, SAMSUNG_LENS_TYPE};
    use crate::rm_lens_builtin::Parts;
    use crate::rm_lens_builtin::test_files::{
        Entry, adobe, digest, hex, largest_ca_difference, largest_difference, read_framed,
    };

    /// An SRW with IFD0's Make and Model and a maker note of `note` entries (tag,
    /// type, count, data in the file's byte order); offsets in the note count from
    /// its start.
    fn srw(little: bool, model: &str, note: &[Entry]) -> Vec<u8> {
        let int = |v: u32| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let short = |v: u16| {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let directory = |at: usize, entries: &[Entry]| {
            let mut data_at = at + 2 + entries.len() * 12 + 4;
            let (mut out, mut data): (Vec<u8>, Vec<u8>) =
                (short(entries.len() as u16).to_vec(), vec![]);
            for (tag, kind, count, bytes) in entries {
                out.extend(short(*tag));
                out.extend(short(*kind));
                out.extend(int(*count));
                if bytes.len() <= 4 {
                    out.extend(bytes.iter().chain(&[0; 4]).take(4));
                } else {
                    out.extend(int(data_at as u32));
                    data.extend(bytes);
                    data_at += bytes.len();
                }
            }
            out.extend(int(0));
            out.extend(data);
            out
        };
        let note = directory(0, note);
        let make = b"SAMSUNG\0".to_vec();
        let model = [model.as_bytes(), b"\0"].concat();
        let ifd0 = |exif: u32| {
            directory(
                8,
                &[
                    (0x010f, 2, make.len() as u32, make.clone()),
                    (0x0110, 2, model.len() as u32, model.clone()),
                    (0x8769, 4, 1, int(exif).to_vec()),
                ],
            )
        };
        let exif_at = 8 + ifd0(0).len();
        let exif =
            |at: u32| directory(exif_at, &[(0x927c, 7, note.len() as u32, int(at).to_vec())]);
        let note_at = exif_at + exif(0).len();
        let mut out = if little { b"II*\0" } else { b"MM\0*" }.to_vec();
        out.extend(int(8));
        out.extend(ifd0(exif_at as u32));
        out.extend(exif(note_at as u32));
        out.extend(note);
        out
    }

    /// A Samsung maker note's LensType (0xA003), EncryptionKey (0xA020), Distortion
    /// (0xA050) and ChromaticAberration (0xA051), as stored.
    struct SamsungTags<'a> {
        little: bool,
        model: &'a str,
        lens: &'a str,
        key: &'a str,
        distortion: &'a str,
        ca: &'a str,
    }

    const BIG_KEY: &str = "00 00 01 31 00 00 00 48 00 00 02 e1 00 00 01 c8 00 00 01 1a 00 00 01 33
        00 00 02 07 00 00 02 d4 00 00 00 0d 00 00 01 f9 00 00 00 c1";

    /// 2013-05-08-194524__sam6589.srw (raw.pixls.us, NX200, NX 16mm F2.4).
    const NX200: SamsungTags<'static> = SamsungTags {
        little: false,
        model: "NX200",
        lens: "00 08",
        key: BIG_KEY,
        distortion: "ff ff fe d0 ff ff ff b8 ff ff db 9f ff ff fe 2c 00 00 05 6e ff ff fe ba
            ff ff fd f9 ff ff fd 2c",
        ca: "00 07 03 c3 05 6c 0a 97 0a c7 0b 62 0c c4 0e e1 29 ee 27 27 29 13 27 db 28 4b
            27 62 29 fb 28 d7 28 29 28 42 29 16 29 e3 27 1c 27 1c",
    };

    /// The default crop of the NX200 file's 5530 × 3694 decoded image.
    const NX200_SIZE: [f64; 2] = [5472., 3648.];

    /// RAW_SAMSUNG_EX2F.SRW (raw.pixls.us, EX2F).
    const EX2F: SamsungTags<'static> = SamsungTags {
        little: false,
        model: "EX2F",
        lens: "00 00",
        key: BIG_KEY,
        distortion: "ff ff fe d0 ff ff ff b8 ff ff f8 8d ff ff fe 2d ff ff ff ae ff ff fe ba
            ff ff fd f9 ff ff fd 2c",
        ca: "00 07 02 77 02 d3 06 b2 06 df 07 77 08 d5 0a ef 29 e1 27 1a 29 06 27 ce 28 3e
            27 55 29 ee 28 f2 28 44 28 5d 29 31 29 fe 27 37 27 37",
    };

    /// SAM_0295.SRW (raw.pixls.us, NX mini, NX-M 9mm F3.5): a little-endian file.
    const NX_MINI: SamsungTags<'static> = SamsungTags {
        little: true,
        model: "NX mini",
        lens: "00 00 00 00",
        key: "31 01 00 00 48 00 00 00 e1 02 00 00 c8 01 00 00 1a 01 00 00 33 01 00 00
            07 02 00 00 d4 02 00 00 0d 00 00 00 f9 01 00 00 c1 00 00 00",
        distortion: "d0 fe ff ff b8 ff ff ff 46 dc ff ff 2c fe ff ff e4 02 00 00 ba fe ff ff
            f9 fd ff ff 2c fd ff ff",
        ca: "07 00 c3 03 6c 05 97 0a c7 0a 62 0b c4 0c e1 0e ea 29 23 27 0f 29 d7 27 47 28
            5e 27 f7 29 d8 28 2a 28 43 28 17 29 e4 29 1d 27 1d 27",
    };

    fn samsung_file(s: &SamsungTags) -> Vec<u8> {
        let lens = hex(s.lens);
        let note = [
            (SAMSUNG_LENS_TYPE, 3, lens.len() as u32 / 2, lens),
            (SAMSUNG_KEY, 4, 11, hex(s.key)),
            (SAMSUNG_DISTORTION, 9, 8, hex(s.distortion)),
            (SAMSUNG_CHROMATIC, 3, 22, hex(s.ca)),
        ];
        srw(s.little, s.model, &note)
    }

    /// Against the warp Adobe DNG Converter 18.7 writes for the NX200 file: radius 1
    /// at the corner of its 5528 × 3670 ActiveArea farthest from the default crop's
    /// centre, (2771, 1839).
    #[test]
    fn reads_samsung_corrections_as_adobe_does() {
        let c = read_framed(&samsung_file(&NX200), NX200_SIZE).unwrap();
        assert!(c.default_on && c.vignetting.is_none());
        assert_eq!(c.source, "Samsung built-in");
        assert_eq!(
            c.always,
            Some(Parts {
                distortion: true,
                chromatic: true,
                vignetting: false,
            })
        );
        let warp = [
            [
                1.0005000000000004,
                -0.09490109134425251,
                0.020464071708261592,
                0.,
            ],
            [1., -0.094853664512, 0.02045384478587856, 0.],
            [
                0.9999499999999998,
                -0.09484892182877282,
                0.02045282209363186,
                0.,
            ],
        ];
        let unit = 2771f64.hypot(1839.);
        let off = largest_difference(&c, NX200_SIZE, &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
        let (ca, most) = largest_ca_difference(&c, NX200_SIZE, &|ch, px| adobe(warp[ch], unit, px));
        assert!(ca < 0.02, "{ca} px from Adobe's CA");
        assert!(most > 1., "Adobe corrects {most} px");
        // Without the correction the corner is 250 px out.
        assert!(adobe(warp[1], unit, 3300.) < 3300. - 240.);
    }

    /// The EX2F's red and blue scale by 0.75 and 0.25 of their stored change where the
    /// interchangeable-lens bodies' scale by half (radius 1 at the corner farthest from
    /// (2044, 1534) in a 4070 × 3046 ActiveArea).
    #[test]
    fn reads_samsung_compact_chromatic_aberration_as_adobe_does() {
        // The default crop of the 4040 × 3029 decoded image.
        let size = [4000., 3000.];
        let c = read_framed(&samsung_file(&EX2F), size).unwrap();
        let warp = [
            [0.999775, -0.07639658330031347, 0.0008529113060220778, 0.],
            [1., -0.0764137764, 0.00085310325424928, 0.],
            [1.00065, -0.07646344535466416, 0.000853657771376213, 0.],
        ];
        let unit = 2044f64.hypot(1534.);
        let off = largest_difference(&c, size, &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
    }

    #[test]
    fn reads_little_endian_samsung_notes() {
        // The default crop of the 5544 × 3692 decoded image.
        let size = [5472., 3648.];
        let c = read_framed(&samsung_file(&NX_MINI), size).unwrap();
        let warp = [
            [1.0003, -0.09187401553847731, 0.01219599728573692, 0.],
            [1., -0.09184646159999998, 0.012192339583871994, 0.],
            [1., -0.09184646159999642, 0.012192339583864665, 0.],
        ];
        let unit = 2748f64.hypot(1836.);
        let off = largest_difference(&c, size, &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
    }

    /// Adobe ignores the camera's data for lenses and compacts it has its own profiles
    /// for, and on the first NX bodies.
    #[test]
    fn samsung_data_adobe_does_not_use_is_not_read() {
        for lens in ["00 01", "00 02", "00 03"] {
            let file = samsung_file(&SamsungTags { lens, ..NX200 });
            assert!(read_framed(&file, NX200_SIZE).is_none(), "{lens}");
        }
        for model in ["NX5", "NX10", "NX100", "NX11", "EX1", "WB2000"] {
            let file = samsung_file(&SamsungTags { model, ..NX200 });
            assert!(read_framed(&file, NX200_SIZE).is_none(), "{model}");
        }
    }

    /// A distortion model that is zero at the centre is not one; Adobe keeps the CA.
    #[test]
    fn samsung_chromatic_aberration_without_distortion() {
        // The constant's mantissa 1 stored as 0.
        let distortion = NX200.distortion.replacen("ff ff fe d0", "ff ff fe cf", 1);
        let tags = SamsungTags {
            distortion: &distortion,
            ..NX200
        };
        let c = read_framed(&samsung_file(&tags), NX200_SIZE).unwrap();
        assert!(c.distortion.is_none());
        assert_eq!(
            c.always,
            Some(Parts {
                chromatic: true,
                ..Parts::default()
            })
        );
        let [red, blue] = c.chromatic.as_ref().unwrap();
        assert!((red.eval(0.5) - 1.0005).abs() < 1e-6);
        assert!((blue.eval(0.9) - 0.99995).abs() < 1e-6);
    }

    /// Today's reader, pinned as `todays_readers_are_pinned` pins the others: a
    /// change meant to move it updates the number; any other change fails.
    #[test]
    fn todays_reader_is_pinned() {
        assert_eq!(
            digest(&read_framed(&samsung_file(&NX200), NX200_SIZE)),
            9_911_406_363_803_893_714
        );
    }
}
