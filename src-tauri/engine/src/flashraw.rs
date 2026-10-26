//! FlashRaw's samples in rawler's image: the RawImage rawler's own decode
//! makes, with the sensor data unpacked by FlashRaw. Ours, on FlashRaw 0.1.1
//! (github.com/ybouane/flashraw, MIT OR Apache-2.0).
//!
//! WHY BOTH
//!
//! FlashRaw unpacks a RAW's sensor data faster than rawler, on every core
//! (1.5 to 3.4 times, measured 2026-10-26 on 20 files). It does not read the
//! camera's own white level (it uses 16383 where rawler has Canon's 14008 to
//! 14351, Sony's 15360, Nikon's 15892), and it carries colour matrices of its
//! own for some bodies, so on its own it would change how photos look. So
//! rawler reads everything but the samples, in its metadata pass (3 to 30 ms,
//! the same values as its full decode on all 20 files), and FlashRaw's samples
//! go into that image.
//!
//! ONLY WHERE MEASURED
//!
//! `MEASURED` holds the format and compression pairs whose samples were
//! compared with rawler's on real files and found identical, so the app's
//! develop of them is identical too. Everything else stays with rawler:
//!
//! - DNG: measured when rawler's metadata pass still decoded a DNG's samples,
//!   which made FlashRaw on top slower. That pass skips them since the fork's
//!   930e6ce2 (RapidRAW 9dbcc565); DNG stays with rawler until measured again.
//! - Sony: no gain on lossless, slower and not identical on compressed.
//! - Canon sRAW: FlashRaw converts it differently (dE2000 1.2 to 2.5).
//! - Nikon: one count off on three samples in four, which tips photosites
//!   across the clipping point (dE2000 0.1 on average, up to 47 there).
//! - Any format nobody has compared yet.
//!
//! And a file rawler would measure the black of from its own masked pixels
//! (no black in its camera table or in the file) stays with rawler, which
//! measures it.
//!
//! `None` from `decode` means "decode it with rawler", never an error.

use std::panic::AssertUnwindSafe;

use flashraw::{
    Compression, CorruptionPolicy, DecodeOptions, FlashRaw, OutputFormat, RawFormat, RawPixelData,
};
use rawler::decoders::{Decoder, RawDecodeParams};
use rawler::rawimage::{BlackLevel, RawImage, RawImageData, RawPhotometricInterpretation};
use rawler::rawsource::RawSource;

/// The pairs measured identical to rawler on real files. One more needs the
/// same comparison first (the app's `mods::raw_decoder` sample test).
const MEASURED: [(RawFormat, Compression); 6] = [
    (RawFormat::Cr3, Compression::CanonCrx),
    (RawFormat::Cr2, Compression::LosslessJpeg),
    (RawFormat::Raf, Compression::FujiCompressed),
    (RawFormat::Rw2, Compression::PanasonicV8),
    (RawFormat::Orf, Compression::OlympusCompressed),
    (RawFormat::Pef, Compression::PentaxHuffman),
];

fn measured(format: Option<RawFormat>, compression: Option<Compression>) -> bool {
    matches!((format, compression), (Some(f), Some(c)) if MEASURED.contains(&(f, c)))
}

/// Would `decode` give this file FlashRaw's samples?
pub fn handles(bytes: &[u8], source: &RawSource, decoder: &dyn Decoder) -> bool {
    guarded(|| without_samples(&FlashRaw::open(bytes).ok()?, source, decoder)).is_some()
}

/// rawler's image of the file with FlashRaw's samples, or `None` to decode
/// it with rawler.
pub fn decode(bytes: &[u8], source: &RawSource, decoder: &dyn Decoder) -> Option<RawImage> {
    guarded(|| {
        let file = FlashRaw::open(bytes).ok()?;
        let mut raw = without_samples(&file, source, decoder)?;
        let frame = file
            .decode(DecodeOptions {
                output: OutputFormat::RawU16,
                // A cut-short file is rawler's to refuse or repair, as it is today.
                corruption_policy: CorruptionPolicy::Strict,
                ..Default::default()
            })
            .ok()?;
        if (frame.width as usize, frame.height as usize) != (raw.width, raw.height) {
            return None;
        }
        let RawPixelData::U16(samples) = frame.data else {
            return None;
        };
        raw.data = RawImageData::Integer(samples);
        Some(raw)
    })
}

/// rawler's image without its samples, when FlashRaw can supply them.
fn without_samples(file: &FlashRaw, source: &RawSource, decoder: &dyn Decoder) -> Option<RawImage> {
    let meta = file.metadata();
    if !measured(meta.format, meta.compression) {
        return None;
    }
    let mut raw = decoder
        .raw_image(source, &RawDecodeParams::default(), true)
        .ok()?;
    let mosaic = raw.cpp == 1 && matches!(raw.photometric, RawPhotometricInterpretation::Cfa(_));
    // rawler's metadata pass leaves the black at zero where its full decode
    // measures it from the masked pixels.
    let measures_black =
        raw.camera.make_blacklevel(raw.cpp).is_none() && raw.blacklevel == BlackLevel::default();
    if !mosaic || measures_black {
        return None;
    }
    // The metadata pass reserves a full buffer it never fills: given back
    // before FlashRaw makes its own, so a decode holds one, as rawler's does.
    raw.data = RawImageData::Integer(Vec::new());
    Some(raw)
}

/// A panic in either decoder means rawler decodes the file, as it would have.
fn guarded<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_measured_pairs() {
        assert!(measured(Some(RawFormat::Cr3), Some(Compression::CanonCrx)));
        assert!(measured(
            Some(RawFormat::Raf),
            Some(Compression::FujiCompressed)
        ));
        assert!(!measured(
            Some(RawFormat::Nef),
            Some(Compression::NikonHuffman)
        ));
        // Canon sRAW is a CR2 too.
        assert!(!measured(
            Some(RawFormat::Cr2),
            Some(Compression::CanonSraw)
        ));
        assert!(!measured(
            Some(RawFormat::Dng),
            Some(Compression::LosslessJpeg)
        ));
        assert!(!measured(
            Some(RawFormat::Arw),
            Some(Compression::SonyLossless)
        ));
        assert!(!measured(Some(RawFormat::Arw), Some(Compression::SonyArw2)));
        assert!(!measured(
            Some(RawFormat::Rw2),
            Some(Compression::PanasonicV7)
        ));
        assert!(!measured(None, Some(Compression::CanonCrx)));
        assert!(!measured(Some(RawFormat::Cr3), None));
    }
}
