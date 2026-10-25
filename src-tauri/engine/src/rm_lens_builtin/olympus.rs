//! Olympus's and OM System's built-in lens correction, read from an ORF.
//!
//! Micro Four Thirds lenses rely on software distortion correction, and the
//! bodies write the model into every ORF's maker note: its ImageProcessing
//! directory (0x2040), tags 0x150A (distortion: k1, k2, k3 and a scale s,
//! floats) and 0x150C (lateral CA: three coefficients for red, then three for
//! blue, floats). With radius 1 at the half diagonal of the camera's default
//! crop, an output radius r samples green at
//! s·r·(1 + k1 (s·r)² + k2 (s·r)⁴ + k3 (s·r)⁶), and red and blue at that radius
//! times 1 + c0 + c1 ρ² + c2 ρ⁴, ρ being green's source radius.
//!
//! Adobe applies it as the photo's lens profile, written into its DNGs as
//! their warp or, for newer bodies, as the "Camera Settings" profile, so it
//! starts on, and the parts the file stores are always the camera's. There is
//! no vignetting: Lightroom corrects none from the file, and a lens profile's
//! corrects it here.
//!
//! FROM RAWMAKASE
//!
//! RAWmakase @ af35e25 (MIT):
//! - `crates/rawmakase-model/src/lens/embedded.rs`: the Olympus reader
//!   (`olympus`, `floats`), its model, and its tests.
//!
//! WHAT CHANGED IN THE COPY
//!
//! - The maker note is read with rawler's TIFF reader in place of
//!   `rawmakase-tiff`, from the file's bytes rather than its path, its
//!   headers and offsets as RAWmakase reads them. rawler's own
//!   `orf::parse_makernote` is not used: it takes ImageProcessing's offset
//!   from the bytes the older notes' inline directory starts with, and
//!   asserts on an OM System note's byte order.
//! - Radius 1 is the developed image's half diagonal, as in every reader
//!   here. rawler crops an ORF to its default crop, so the model's unit (the
//!   default crop's half diagonal) is that same radius, and the correction is
//!   centred on the default crop, as Adobe's is, where RAWmakase centres it on
//!   the decoded image (8 px apart on an E-M10 Mark III).
//! - `always` names the parts the file stores.

use std::io::Cursor;

use rawler::bits::Endian;
use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{Entry, GenericTiffReader, IFD, Value};
use rawler::tags::ExifTag;

use super::LensCorrection;
use super::fit::maker_model;

/// The maker note's ImageProcessing directory, and its distortion and CA.
pub(super) const IMAGE_PROCESSING: u16 = 0x2040;
pub(super) const OLYMPUS_DISTORTION: u16 = 0x150a;
pub(super) const OLYMPUS_CHROMATIC: u16 = 0x150c;

/// TIFF types the ImageProcessing entry is written as: an offset (LONG or
/// IFD), or the directory inline (UNDEFINED).
const LONG: u16 = 4;
const UNDEFINED: u16 = 7;
const IFD_OFFSET: u16 = 13;

// RAWmakase af35e25: crates/rawmakase-model/src/lens/embedded.rs
/// The values of a FLOAT entry.
fn floats(e: &Entry) -> Option<Vec<f64>> {
    let Value::Float(v) = &e.value else {
        return None;
    };
    let v: Vec<f64> = v.iter().map(|x| f64::from(*x)).collect();
    v.iter().all(|x| x.is_finite()).then_some(v)
}

/// The model in an ORF's maker note; `None` for a file that is not an ORF,
/// or one whose note has none. `size_px` is the developed image's.
pub(super) fn read(file: &[u8], size_px: &dyn Fn() -> Option<[f64; 2]>) -> Option<LensCorrection> {
    if ![b"IIRO", b"IIRS", b"MMOR"]
        .iter()
        .any(|h| file.starts_with(*h))
    {
        return None;
    }
    let tiff = GenericTiffReader::new_with_buffer(file, 0, 0, Some(0)).ok()?;
    let note = tiff
        .find_first_ifd_with_tag(ExifTag::MakerNotes)?
        .get_entry(ExifTag::MakerNotes)?;
    let Value::Undefined(head) = &note.value else {
        return None;
    };
    let at = note.offset()?;
    // RAWmakase af35e25: crates/rawmakase-model/src/lens/embedded.rs
    // Offsets in the note count from its start, after its own byte order mark.
    let (start, source) = if head.starts_with(b"OLYMPUS\0") {
        (8, "Olympus built-in")
    } else if head.starts_with(b"OM SYSTEM\0\0\0") {
        (12, "OM System built-in")
    } else {
        return None;
    };
    let endian = match head.get(start..start + 2)? {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    let base = u32::try_from(at).ok()?;
    let mut reader = Cursor::new(file);
    let note_ifd = |reader: &mut Cursor<&[u8]>, offset: u32| {
        base.checked_add(offset)?;
        IFD::new(reader, offset, base, 0, endian, &[]).ok()
    };
    let main = note_ifd(&mut reader, start as u32 + 4)?;
    let e = main.get_entry(IMAGE_PROCESSING)?;
    // The entry's four value bytes are the directory's offset, whatever its
    // type: an offset's own value, or, where older notes store the directory
    // inline, the offset of that data.
    let field = if e.value.byte_size() > 4 {
        u32::try_from(e.offset()?).ok()?
    } else {
        let field = at.checked_add(e.offset()?)?;
        endian.read_u32(file.get(field..field + 4)?, 0)
    };
    let processing = match e.value.value_type() {
        LONG | UNDEFINED | IFD_OFFSET => field,
        _ => return None,
    };
    let ip = note_ifd(&mut reader, processing)?;
    // RAWmakase af35e25: crates/rawmakase-model/src/lens/embedded.rs
    let d = ip
        .get_entry(OLYMPUS_DISTORTION)
        .and_then(floats)
        .filter(|d| d.len() == 4)?;
    let ca: Option<[f64; 6]> = ip
        .get_entry(OLYMPUS_CHROMATIC)
        .and_then(floats)
        .and_then(|c| c.try_into().ok());
    let s = if d[3] > 0. { d[3] } else { 1. };
    let k = [d[0], d[1], d[2]];
    let green = |r: f64| {
        let x = s * r;
        let x2 = x * x;
        Some(x * (1. + x2 * (k[0] + x2 * (k[1] + x2 * k[2]))))
    };
    // Red's and blue's scale at green's source radius ρ.
    let scale = |at: usize, rho: f64| {
        ca.map_or(1., |c| {
            1. + c[at] + rho * rho * (c[at + 1] + rho * rho * c[at + 2])
        })
    };
    let (red, blue) = (|rho| scale(0, rho), |rho| scale(3, rho));
    let chromatic = ca.is_some().then_some([&red as &dyn Fn(f64) -> f64, &blue]);
    let size = size_px()?;
    // The default crop's half diagonal: the developed image is the default crop.
    let unit = size[0].hypot(size[1]) * 0.5;
    let (distortion, chromatic) = maker_model(size, unit, &green, chromatic);
    let mut c = LensCorrection {
        source: source.into(),
        // Adobe applies it as the photo's lens profile: written into its DNGs as
        // their warp, or for newer bodies as the "Camera Settings" profile.
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
    use super::{IMAGE_PROCESSING, OLYMPUS_CHROMATIC, OLYMPUS_DISTORTION};
    use crate::rm_lens_builtin::Parts;
    use crate::rm_lens_builtin::test_files::{
        Entry, adobe, digest, directory, hex, largest_difference, read_framed,
    };

    /// An ORF whose maker note's ImageProcessing directory holds `processing`.
    fn orf(processing: &[Entry]) -> Vec<u8> {
        orf_with(b"OLYMPUS\0II\x03\0", processing)
    }

    /// An ORF whose maker note starts with `header`, ending in a byte order mark and
    /// two version bytes.
    fn orf_with(header: &[u8], processing: &[Entry]) -> Vec<u8> {
        let main = header.len();
        let mut note = header.to_vec();
        let at = (main + 18) as u32;
        note.extend(directory(
            main,
            &[(IMAGE_PROCESSING, 13, 1, at.to_le_bytes().to_vec())],
        ));
        note.extend(directory(at as usize, processing));
        let mut out = b"IIRO\x08\0\0\0".to_vec();
        out.extend(directory(
            8,
            &[(0x8769, 4, 1, 26u32.to_le_bytes().to_vec())],
        ));
        out.extend(directory(
            26,
            &[(0x927c, 7, note.len() as u32, note.clone())],
        ));
        out
    }

    /// PA080192.ORF's tags (raw.pixls.us, E-M10 Mark III, M.17mm F1.8).
    fn em10_file() -> Vec<u8> {
        let distortion = hex("97 f4 9a bd 66 06 cb bb b3 e4 b7 3b 00 00 80 3f");
        let ca = hex("00 98 2d 3a 00 20 94 b9 00 80 b9 b9 00 58 20 ba 00 48 80 3a 00 a0 64 39");
        orf(&[
            (OLYMPUS_DISTORTION, 11, 4, distortion),
            (OLYMPUS_CHROMATIC, 11, 6, ca),
        ])
    }

    /// The default crop of PA080192.ORF's 4640 × 3472 decoded image.
    const EM10_SIZE: [f64; 2] = [4608., 3456.];

    /// PA080192.ORF against the warp Adobe DNG Converter 18.7 writes for it (radius 1
    /// at the 4640 × 3472 image's corner farthest from its centre, (2312, 1736)).
    #[test]
    fn reads_olympus_distortion_and_chromatic_aberration_as_adobe_does() {
        let file = em10_file();
        let size = EM10_SIZE;
        let c = read_framed(&file, size).unwrap();
        assert!(c.default_on && c.vignetting.is_none());
        assert_eq!(c.source, "Olympus built-in");
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
                1.000663065655637,
                -0.07727914192099739,
                -0.006663217130000866,
                0.005985386689871972,
            ],
            [
                1.000000000000001,
                -0.0769286992183596,
                -0.006405051999955553,
                0.0058986243274408824,
            ],
            [
                0.999387503381258,
                -0.07587444557829748,
                -0.006451880311619311,
                0.005871129977034196,
            ],
        ];
        let unit = 2328f64.hypot(1736.);
        let off = largest_difference(&c, size, &|ch, px| adobe(warp[ch], unit, px));
        assert!(off < 0.1, "{off} px from Adobe's warp");
        // Without the correction the corner is 220 px out.
        assert!(adobe(warp[1], unit, 2880.) < 2880. - 200.);
        // The app's `read` takes the size from rawler, which does not open
        // this file: no size, no correction.
        assert!(crate::rm_lens_builtin::read(&file).is_none());
    }

    /// OM System bodies start their note with a longer header.
    #[test]
    fn reads_om_system_notes() {
        let distortion = [-0.02f32, 0.01, -0.003, 1.]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let file = orf_with(
            b"OM SYSTEM\0\0\0II\x04\0",
            &[(OLYMPUS_DISTORTION, 11, 4, distortion)],
        );
        // The default crop of the 5240 × 3912 decoded image.
        let c = read_framed(&file, [5184., 3888.]).unwrap();
        assert_eq!(c.source, "OM System built-in");
        assert!(c.chromatic.is_none());
        assert_eq!(
            c.always,
            Some(Parts {
                distortion: true,
                ..Parts::default()
            })
        );
        // Radius 1 of the model is the default crop's half diagonal, 3240 px.
        let half = 5184f32.hypot(3888.) / 2.;
        let at = |px: f32| c.distortion.as_ref().unwrap().eval(px / half);
        let model = |x: f32| 1. + x * x * (-0.02 + x * x * (0.01 - 0.003 * x * x));
        assert!((at(3240.) - model(1.)).abs() < 1e-5);
        assert!((at(1620.) - model(0.5)).abs() < 1e-5);
    }

    #[test]
    fn olympus_note_without_corrections_has_none() {
        let zero = |n: usize| [0, 0, 0, 0].repeat(n);
        let one = [&zero(3)[..], &1f32.to_le_bytes()].concat();
        let size = [4608., 3456.];
        let file = orf(&[
            (OLYMPUS_DISTORTION, 11, 4, one),
            (OLYMPUS_CHROMATIC, 11, 6, zero(6)),
        ]);
        assert!(read_framed(&file, size).is_none());
        assert!(read_framed(&orf(&[(0x0100, 3, 2, vec![0; 4])]), size).is_none());
    }

    /// Older notes store the ImageProcessing directory inline, as the entry's
    /// data, where newer ones store its offset.
    #[test]
    fn reads_an_inline_image_processing_directory() {
        let distortion: Vec<u8> = [-0.02f32, 0.01, -0.003, 1.]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let header = b"OLYMPUS\0II\x03\0";
        let main = header.len();
        // The directory is the entry's data, right after the main directory.
        let inline = directory(main + 18, &[(OLYMPUS_DISTORTION, 11, 4, distortion)]);
        let mut note = header.to_vec();
        note.extend(directory(
            main,
            &[(IMAGE_PROCESSING, 7, inline.len() as u32, inline)],
        ));
        let mut file = b"IIRO\x08\0\0\0".to_vec();
        file.extend(directory(
            8,
            &[(0x8769, 4, 1, 26u32.to_le_bytes().to_vec())],
        ));
        file.extend(directory(
            26,
            &[(0x927c, 7, note.len() as u32, note.clone())],
        ));
        let c = read_framed(&file, [5184., 3888.]).unwrap();
        assert!(c.distortion.is_some() && c.chromatic.is_none());
    }

    /// Today's reader, pinned as `todays_readers_are_pinned` pins the others: a
    /// change meant to move it updates the number; any other change fails.
    #[test]
    fn todays_reader_is_pinned() {
        assert_eq!(
            digest(&read_framed(&em10_file(), EM10_SIZE)),
            17_235_185_407_300_217_138
        );
    }
}
