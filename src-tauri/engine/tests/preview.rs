//! A preview that half the photo still covers develops at half size, and has
//! to look like the full-size develop made small: same size, same colour,
//! highlights recovered the same, the crop in the same place.

mod common;

use argentum_engine::rawler::imgop::{Dim2, Point, Rect};
use argentum_engine::rawler::rawimage::RawPhotometricInterpretation;
use argentum_engine::tools::rt_capture_sharpen;
use argentum_engine::{Pipeline, Rgb, Size, Source};

fn ran(p: &Pipeline) -> Vec<&'static str> {
    p.last_render()
        .iter()
        .filter(|s| s.ran)
        .map(|s| s.name)
        .collect()
}

/// `rgb` box-averaged to `w` x `h`.
fn made_small(rgb: &Rgb, w: usize, h: usize) -> Vec<[f32; 3]> {
    let span = |o: usize, out: usize, input: usize| {
        let a = o * input / out;
        a..((o + 1) * input / out).max(a + 1).min(input)
    };
    let mut out = Vec::with_capacity(w * h);
    for oy in 0..h {
        for ox in 0..w {
            let (ys, xs) = (span(oy, h, rgb.height), span(ox, w, rgb.width));
            let mut sum = [0.0f64; 3];
            for y in ys.clone() {
                for p in &rgb.pixels[y * rgb.width + xs.start..y * rgb.width + xs.end] {
                    (0..3).for_each(|c| sum[c] += p[c] as f64);
                }
            }
            let n = (ys.len() * xs.len()) as f64;
            out.push(sum.map(|s| (s / n) as f32));
        }
    }
    out
}

#[test]
fn a_half_size_preview_is_the_full_develop_made_small() {
    let mut cropped = common::scene(1280, 960);
    cropped.active_area = Some(Rect::new(Point::new(24, 16), Dim2::new(1232, 928)));
    cropped.crop_area = Some(Rect::new(Point::new(40, 28), Dim2::new(1180, 880)));
    // An active area starting on an odd photosite: the mosaic's pattern
    // shifts under it.
    let mut odd = common::scene(1280, 960);
    odd.active_area = Some(Rect::new(Point::new(13, 9), Dim2::new(1250, 940)));
    let mut sources = vec![
        (
            "synthetic".to_string(),
            common::source(1280, 960),
            (300, 300),
        ),
        (
            "synthetic, cropped".to_string(),
            Source::new(7, cropped),
            (300, 300),
        ),
        (
            "synthetic, odd active area".to_string(),
            Source::new(8, odd),
            (300, 300),
        ),
    ];
    for path in common::raws("a_half_size_preview_is_the_full_develop_made_small") {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        sources.push((name, common::open(&path), (1600, 1600)));
    }

    for (name, source, (max_w, max_h)) in sources {
        let bayer = source.raw.cpp == 1
            && matches!(&source.raw.photometric, RawPhotometricInterpretation::Cfa(c) if c.cfa.width == 2);
        if !bayer {
            eprintln!("  {name:<40} not a Bayer mosaic: it develops at full size");
            continue;
        }
        let mut p = Pipeline::standard().unwrap();
        p.set::<rt_capture_sharpen::RtCaptureSharpen>(rt_capture_sharpen::Settings {
            capture: false,
            ..Default::default()
        })
        .unwrap();
        let full = p.render(&source, Size::Full).unwrap();
        let full = full.as_rgb().unwrap();
        let preview = p.render(&source, Size::Fit(max_w, max_h)).unwrap();
        assert!(
            ran(&p).contains(&"develop_half") && !ran(&p).contains(&"develop"),
            "{name}: {:?}",
            ran(&p)
        );
        let preview = preview.as_rgb().unwrap();
        let fits = |v: usize| {
            v as f64 * (max_w as f64 / full.width as f64).min(max_h as f64 / full.height as f64)
        };
        assert_eq!(
            (preview.width, preview.height),
            (
                fits(full.width).round() as usize,
                fits(full.height).round() as usize
            ),
            "{name}: the size the full develop would fit at"
        );
        assert_eq!(preview.full_width, full.width, "{name}");

        // Against the full develop box-averaged to the same size. Pixel for
        // pixel the two differ by the photo's noise and by where rawler's
        // camera matrix clips a value - per photosite in one, on averages in
        // the other, as in any preview made below full size. What would be
        // wrong shows elsewhere. The colour: each channel's mean. The crop:
        // the shift that matches best. The tone: the pictures averaged over
        // blocks of 8 x 8. The highlights: the brightest of those.
        let small = made_small(full, preview.width, preview.height);
        let means = |v: &[[f32; 3]]| {
            let mut m = [0.0f64; 3];
            v.iter()
                .for_each(|p| (0..3).for_each(|c| m[c] += p[c] as f64));
            m.map(|s| s / v.len() as f64)
        };
        let (a, b) = (means(&preview.pixels), means(&small));
        let colour = (0..3)
            .map(|c| (a[c] / b[c] - 1.0).abs())
            .fold(0.0, f64::max);
        let shift = best_shift(&preview.pixels, &small, preview.width, preview.height);
        let blocks = |v: &[[f32; 3]]| {
            let rgb = Rgb::new(preview.width, preview.height, v.to_vec());
            made_small(&rgb, preview.width / 8, preview.height / 8)
        };
        let (coarse_a, coarse_b) = (blocks(&preview.pixels), blocks(&small));
        let mut rel: Vec<(f32, f32)> = coarse_a
            .iter()
            .zip(&coarse_b)
            .flat_map(|(a, b)| (0..3).map(move |c| ((a[c] - b[c]).abs() / b[c].max(0.01), b[c])))
            .collect();
        rel.sort_by(|x, y| x.0.total_cmp(&y.0));
        let share = |f: f64| rel[((rel.len() - 1) as f64 * f) as usize].0 as f64;
        let (median, p99) = (share(0.5), share(0.99));
        let brightest = |v: &[[f32; 3]]| v.iter().flatten().fold(0.0f32, |m, &x| m.max(x)) as f64;
        let highlights = (brightest(&coarse_a) / brightest(&coarse_b) - 1.0).abs();
        eprintln!(
            "  {name:<40} {} x {}: colour {:.2}%, shift {shift:?}, tone {:.2}% (median) {:.2}% (99th), brightest {:.2}%",
            preview.width,
            preview.height,
            colour * 100.0,
            median * 100.0,
            p99 * 100.0,
            highlights * 100.0
        );
        // Measured on the synthetic photos and a 32 MP CR3: colour at most
        // 0.21%, tone 0.57% (median) and 5.0% (99th), brightest 1.9%.
        assert!(colour < 0.01, "{name}: the colour moved");
        assert_eq!(shift, (0, 0), "{name}: the crop moved");
        assert!(median < 0.02 && p99 < 0.10, "{name}: the tone moved");
        assert!(highlights < 0.05, "{name}: the highlights moved");
    }
}

/// The shift of `a` against `b`, within three pixels, that matches best.
fn best_shift(a: &[[f32; 3]], b: &[[f32; 3]], w: usize, h: usize) -> (i64, i64) {
    let (w, h) = (w as i64, h as i64);
    let mut best = (f64::MAX, (0, 0));
    for dy in -3i64..=3 {
        for dx in -3i64..=3 {
            let mut d = 0.0f64;
            for y in 4..h - 4 {
                for x in 4..w - 4 {
                    let (p, q) = (a[(y * w + x) as usize], b[((y + dy) * w + x + dx) as usize]);
                    d += (0..3).map(|c| (p[c] - q[c]).abs() as f64).sum::<f64>();
                }
            }
            if d < best.0 {
                best = (d, (dx, dy));
            }
        }
    }
    best.1
}
