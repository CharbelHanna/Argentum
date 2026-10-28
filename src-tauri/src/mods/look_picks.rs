//! Suggested looks: the LUTs the user is likely to put on the open photo,
//! learned from the LUTs they put on photos before. Ours.
//!
//! Every LUT put on a photo in the editor is a pick, kept with the photo's
//! fingerprint: its light, lamp-lit or daylight, from the camera's As Shot
//! white balance and how bright the scene was (its exposure), and eight
//! numbers about its colours. For the open photo, the
//! eight picks nearest to it in the same light vote, and the LUTs with the most
//! votes are suggested, the closer picks deciding a tie. Counting, not a model:
//! it works from the third pick on and downloads nothing.
//!
//! One pick per photo, the last LUT put on it; taking the LUT off takes the
//! pick back. A photo's own pick never counts for it, so its suggestions stay
//! where they are while LUTs are tried on it. The picks are one small file
//! beside Argentum's other settings (`look-picks.json`); no photo is read but
//! the open one.
//!
//! Each pick also keeps where its LUT stood among the photo's three
//! suggestions, so how often they were right can be told (`Book::hits`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::app_state::LoadedImage;

pub const FILE: &str = "look-picks.json";

/// Suggestions start once this many photos have a pick.
pub const MIN_PICKS: usize = 3;

/// How many suggestions the panel shows.
pub const SHOWN: usize = 3;

/// How many of the photo's nearest picks in its light vote.
const VOTERS: usize = 8;

/// How many LUTs a photo is offered: the panel shows the first three it still has.
const OFFERED: usize = 6;

/// Lamp light is warm and dim, and neither alone tells it: a camera's Auto
/// reads 3600-3800 K under lamps but also 3800-4500 K in daylight on leaves,
/// and a bright downlight is shot as fast as shade. So each counts points:
/// one per 400 K of As Shot white balance below 4300 K, one per 2 EV of
/// scene below EV 10 (at ISO 100); lamp-lit from 1.2 points...
const LAMP_PIVOT_KELVIN: f64 = 4300.0;
const KELVIN_PER_POINT: f64 = 400.0;
const LAMP_PIVOT_EV100: f32 = 10.0;
const EV_PER_POINT: f32 = 2.0;
const LAMP_FROM_POINTS: f32 = 1.2;

/// ...and still warm after that white balance (mid-tones' red over blue,
/// log2), as Auto never quite corrects a lamp; daylight it misreads on
/// leaves comes out neutral or cool. On 210 5D Mark II RAWs, 35 under lamps
/// and 175 in daylight, the points alone called 6 daylight frames lamp-lit;
/// with this, none, and every lamp frame stays lamp-lit (warmth 1.09 to 2.30
/// under lamps, -0.21 to 0.96 on the six).
const STILL_WARM_LOG2: f32 = 1.0;

/// Without an As Shot white balance (not a RAW), the photo's own colours
/// stand in, more roughly: mid-tones this much redder than blue count as
/// warm light, 1.5 points, and anything else as -1.5.
const LAMP_ABOVE_WARMTH_LOG2: f32 = 2.2;
const WARMTH_POINTS: f32 = 1.5;

/// About this many pixels along the long edge are measured.
const SAMPLES_LONG_EDGE: usize = 128;

/// How far apart two photos' colours count, value by value: about one spread
/// of each over 118 RAWs, in daylight and under lamps, and how much each
/// weighs. A difference counts for at most three spreads, so one unusual
/// colour does not outweigh the rest.
const SPREAD: [f32; 8] = [2.4, 3.0, 0.04, 0.11, 0.26, 0.05, 0.27, 0.24];
const WEIGHT: [f32; 8] = [1.0, 0.3, 0.8, 1.2, 1.2, 1.2, 0.8, 1.0];

/// What another light adds to the distance: more than most photos' colours differ.
const OTHER_LIGHT: f32 = 6.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Light {
    Lamp,
    Daylight,
}

/// What a photo's colours are like, from about 128 by 85 of its pixels, linear.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Colours {
    /// Red over blue in the mid-tones, log2: how warm the light left it.
    pub warmth_log2: f32,
    /// The median brightness against mid grey (0.18), in stops, -6 to 6.
    pub key_ev: f32,
    /// Mean OkLab chroma: how colourful it is.
    pub chroma: f32,
    /// Shares of the pixels (0 to 1) that are coloured (chroma over 0.04)
    /// and green, sky blue, pink or warm (skin, wood, lamp light).
    pub green: f32,
    pub sky: f32,
    pub pink: f32,
    pub warm: f32,
    /// Share of the pixels close to grey (chroma under 0.03).
    pub neutral: f32,
}

impl Colours {
    fn values(&self) -> [f32; 8] {
        [
            self.warmth_log2,
            self.key_ev,
            self.chroma,
            self.green,
            self.sky,
            self.pink,
            self.warm,
            self.neutral,
        ]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub light: Light,
    pub colours: Colours,
}

impl Fingerprint {
    /// How unlike two photos are: 0 for the same, about 1 per spread apart.
    pub fn distance(&self, other: &Fingerprint) -> f32 {
        let (a, b) = (self.colours.values(), other.colours.values());
        let light = if self.light == other.light {
            0.0
        } else {
            OTHER_LIGHT
        };
        light
            + (0..a.len())
                .map(|i| {
                    let d = ((a[i] - b[i]) / SPREAD[i]).clamp(-3.0, 3.0);
                    WEIGHT[i] * d * d
                })
                .sum::<f32>()
    }
}

/// The open photo's fingerprint; its exposure from the EXIF the app keeps
/// for it once opened.
pub fn fingerprint(loaded: &LoadedImage) -> Fingerprint {
    let (source, _) = crate::file_management::parse_virtual_path(&loaded.path);
    let scene_ev100 =
        crate::exif_processing::read_rrexif_sidecar(&source).and_then(|exif| scene_ev100(&exif));
    measure(loaded, scene_ev100)
}

fn measure(loaded: &LoadedImage, scene_ev100: Option<f32>) -> Fingerprint {
    let colours = colours_of(&loaded.image, loaded.is_raw);
    let as_shot_kelvin = loaded
        .is_raw
        .then_some(loaded.as_shot.white_balance.temperature);
    Fingerprint {
        light: light_of(as_shot_kelvin, scene_ev100, colours.warmth_log2),
        colours,
    }
}

/// Warm and dim enough to be lamp light, and still warm after the camera's
/// white balance; with no exposure known, how warm decides.
fn light_of(as_shot_kelvin: Option<f64>, scene_ev100: Option<f32>, warmth_log2: f32) -> Light {
    let warm_points = match as_shot_kelvin {
        Some(kelvin) => ((LAMP_PIVOT_KELVIN - kelvin) / KELVIN_PER_POINT) as f32,
        None if warmth_log2 > LAMP_ABOVE_WARMTH_LOG2 => WARMTH_POINTS,
        None => -WARMTH_POINTS,
    };
    let dim_points = scene_ev100.map_or(0.0, |ev| (LAMP_PIVOT_EV100 - ev) / EV_PER_POINT);
    if warm_points + dim_points >= LAMP_FROM_POINTS && warmth_log2 >= STILL_WARM_LOG2 {
        Light::Lamp
    } else {
        Light::Daylight
    }
}

/// How bright the scene was, in EV at ISO 100, from the exposure that took
/// it: log2(f-number² / seconds) - log2(ISO / 100).
pub fn scene_ev100(exif: &HashMap<String, String>) -> Option<f32> {
    let number = |key: &str| {
        let text = exif.get(key)?.replace("f/", "").replace(" s", "");
        match text.trim().split_once('/') {
            Some((n, d)) => {
                let (n, d) = (n.trim().parse::<f32>().ok()?, d.trim().parse::<f32>().ok()?);
                (d != 0.0).then(|| n / d)
            }
            None => text.trim().parse::<f32>().ok(),
        }
    };
    let f_number = number("FNumber")?;
    let seconds = number("ExposureTime")?;
    let iso = ["ISOSpeed", "PhotographicSensitivity", "ISOSpeedRatings"]
        .iter()
        .find_map(|key| number(key))?;
    (f_number > 0.0 && seconds > 0.0 && iso > 0.0)
        .then(|| (f_number * f_number / seconds).log2() - (iso / 100.0).log2())
}

fn colours_of(image: &DynamicImage, is_raw: bool) -> Colours {
    let pixel = super::black_white::linear_pixel(image, is_raw);
    let (width, height) = (image.width() as usize, image.height() as usize);
    let step = (width.max(height) / SAMPLES_LONG_EDGE).max(1);
    let rgb: Vec<[f32; 3]> = (0..height)
        .step_by(step)
        .flat_map(|y| (0..width).step_by(step).map(move |x| (x, y)))
        .map(|(x, y)| pixel(x, y))
        .collect();
    colours(&rgb)
}

/// The colours of these linear pixels.
pub fn colours(rgb: &[[f32; 3]]) -> Colours {
    if rgb.is_empty() {
        return Colours::default();
    }
    let luma: Vec<f32> = rgb
        .iter()
        .map(|p| argentum_engine::colour::luma(*p))
        .collect();
    let mut sorted = luma.clone();
    sorted.sort_by(f32::total_cmp);
    let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q).round() as usize];
    let (darkest_tenth, median, near_white) = (at(0.1), at(0.5), at(0.99));

    // Mid-tones: not in the darkest tenth, below clipping.
    let (mut red, mut blue, mut mid) = (0.0f64, 0.0f64, 0usize);
    for (p, &l) in rgb.iter().zip(&luma) {
        if l >= darkest_tenth && p.iter().all(|&v| v < 0.95) {
            red += f64::from(p[0]);
            blue += f64::from(p[2]);
            mid += 1;
        }
    }
    let warmth_log2 = if mid == 0 {
        0.0
    } else {
        let n = mid as f64;
        ((red / n).max(1e-4) / (blue / n).max(1e-4)).log2() as f32
    };

    // Colour, with the photo's near-white at 1 so its exposure does not count.
    let scale = 1.0 / near_white.max(1e-3);
    let (mut chroma, mut green, mut sky, mut pink, mut warm, mut neutral) =
        (0.0f32, 0usize, 0usize, 0usize, 0usize, 0usize);
    for p in rgb {
        let [_, a, b] = oklab(p.map(|v| (v * scale).clamp(0.0, 4.0)));
        let c = a.hypot(b);
        chroma += c;
        if c < 0.03 {
            neutral += 1;
        }
        if c > 0.04 {
            match b.atan2(a).to_degrees().rem_euclid(360.0) {
                h if (100.0..175.0).contains(&h) => green += 1,
                h if (215.0..275.0).contains(&h) => sky += 1,
                h if (20.0..100.0).contains(&h) => warm += 1,
                h if !(20.0..300.0).contains(&h) => pink += 1,
                _ => {}
            }
        }
    }
    let n = rgb.len() as f32;
    let share = |count: usize| count as f32 / n;
    Colours {
        warmth_log2,
        key_ev: (median.max(1e-4) / 0.18).log2().clamp(-6.0, 6.0),
        chroma: chroma / n,
        green: share(green),
        sky: share(sky),
        pink: share(pink),
        warm: share(warm),
        neutral: share(neutral),
    }
}

/// Linear sRGB to OkLab: Björn Ottosson, "A perceptual color space for image
/// processing" (2020), https://bottosson.github.io/posts/oklab/ (public domain).
fn oklab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

// ------------------------------------------------------------------ the picks

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    /// The photo, as the editor names it.
    pub photo: String,
    /// The LUT's path, as the edit keeps it (`lutPath`).
    pub lut: String,
    pub fingerprint: Fingerprint,
    /// Where the LUT stood among the photo's suggestions when it was picked:
    /// 1 to 3, or 0 when it was not one of them; none when there were none yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>,
}

/// Everything kept: whether to learn, and the picks.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Book {
    pub learning: bool,
    pub picks: Vec<Pick>,
}

impl Default for Book {
    fn default() -> Self {
        Self {
            learning: true,
            picks: Vec::new(),
        }
    }
}

#[derive(Serialize, PartialEq, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The LUT's path.
    pub lut: String,
    /// How many of the photo's nearest picks in its light chose it.
    pub votes: usize,
    /// How many photos it was picked for, in all.
    pub picks: usize,
    /// The light its votes came from, or with none, most of its picks.
    pub light: Light,
}

/// What the Looks tab shows for the open photo.
#[derive(Serialize, PartialEq, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Suggested {
    pub learning: bool,
    /// How many photos have a pick.
    pub picks: usize,
    /// The open photo's light.
    pub light: Light,
    /// Most likely first; empty while learning is off or picks are too few.
    pub suggestions: Vec<Suggestion>,
    /// Of the picks made while there were suggestions, how many took one of
    /// the three shown (`right`) out of how many (`of`).
    pub right: usize,
    pub of: usize,
}

impl Book {
    /// The LUTs this photo is likely to get, most likely first.
    pub fn suggest(&self, photo: &str, here: &Fingerprint) -> Vec<Suggestion> {
        if !self.learning || self.picks.len() < MIN_PICKS {
            return Vec::new();
        }
        let mut near: Vec<(&Pick, f32)> = self
            .picks
            .iter()
            .filter(|p| p.photo != photo)
            .map(|p| (p, here.distance(&p.fingerprint)))
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));

        let mut votes: HashMap<&str, usize> = HashMap::new();
        for (pick, _) in near
            .iter()
            .filter(|(p, _)| p.fingerprint.light == here.light)
            .take(VOTERS)
        {
            *votes.entry(pick.lut.as_str()).or_default() += 1;
        }
        // Each pick also counts by how near it is, to settle a tie.
        let mut closeness: HashMap<&str, f32> = HashMap::new();
        for (pick, distance) in &near {
            *closeness.entry(pick.lut.as_str()).or_default() += (-distance / 4.0).exp();
        }

        let mut luts: Vec<&str> = closeness.keys().copied().collect();
        if !votes.is_empty() {
            luts.retain(|lut| votes.contains_key(lut));
        }
        let votes_for = |lut: &str| votes.get(lut).copied().unwrap_or(0);
        luts.sort_by(|a, b| {
            votes_for(b)
                .cmp(&votes_for(a))
                .then(closeness[b].total_cmp(&closeness[a]))
                .then(a.cmp(b))
        });
        luts.into_iter()
            .take(OFFERED)
            .map(|lut| {
                let picked: Vec<&Pick> = near
                    .iter()
                    .map(|(p, _)| *p)
                    .filter(|p| p.lut == lut)
                    .collect();
                let light = if votes_for(lut) > 0 {
                    here.light
                } else {
                    let lamp = picked
                        .iter()
                        .filter(|p| p.fingerprint.light == Light::Lamp)
                        .count();
                    if lamp * 2 > picked.len() {
                        Light::Lamp
                    } else {
                        Light::Daylight
                    }
                };
                Suggestion {
                    lut: lut.to_string(),
                    votes: votes_for(lut),
                    picks: picked.len(),
                    light,
                }
            })
            .collect()
    }

    /// The LUT was put on the photo: its pick, in place of any it had.
    pub fn record(&mut self, photo: &str, lut: &str, fingerprint: Fingerprint) {
        if !self.learning {
            return;
        }
        let shown = self.suggest(photo, &fingerprint);
        let rank = (!shown.is_empty()).then(|| {
            shown
                .iter()
                .take(SHOWN)
                .position(|s| s.lut == lut)
                .map_or(0, |i| i as u8 + 1)
        });
        self.forget(photo);
        self.picks.push(Pick {
            photo: photo.to_string(),
            lut: lut.to_string(),
            fingerprint,
            rank,
        });
    }

    /// The photo's LUT was taken off: its pick goes too.
    pub fn forget(&mut self, photo: &str) {
        self.picks.retain(|p| p.photo != photo);
    }

    /// Of the picks made while there were suggestions, how many took one of
    /// the three shown, and out of how many.
    pub fn hits(&self) -> (usize, usize) {
        let ranked: Vec<u8> = self.picks.iter().filter_map(|p| p.rank).collect();
        let right = ranked.iter().filter(|&&r| r > 0).count();
        (right, ranked.len())
    }

    pub fn suggested(&self, photo: &str, here: &Fingerprint) -> Suggested {
        let (right, of) = self.hits();
        Suggested {
            learning: self.learning,
            picks: self.picks.len(),
            light: here.light,
            suggestions: self.suggest(photo, here),
            right,
            of,
        }
    }
}

// ------------------------------------------------------------------ the file

/// One change at a time: each reads the file, changes it and writes it back.
static WRITING: Mutex<()> = Mutex::new(());

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// The picks kept in `dir`; none when the file is missing or unreadable.
pub fn load(dir: &Path) -> Book {
    std::fs::read_to_string(path(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Change the picks kept in `dir` and write them back, whole: a new file
/// renamed over the old, so a crash leaves one or the other, never half.
pub fn change<T>(dir: &Path, change: impl FnOnce(&mut Book) -> T) -> Result<T, String> {
    let _one_at_a_time = WRITING.lock().map_err(|e| e.to_string())?;
    let mut book = load(dir);
    let out = change(&mut book);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string(&book).map_err(|e| e.to_string())?;
    let fresh = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&fresh, text).map_err(|e| e.to_string())?;
    std::fs::rename(&fresh, path(dir)).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(light: Light, warmth_log2: f32, green: f32, sky: f32) -> Fingerprint {
        Fingerprint {
            light,
            colours: Colours {
                warmth_log2,
                green,
                sky,
                ..Colours::default()
            },
        }
    }

    fn book(picks: &[(&str, &str, Fingerprint)]) -> Book {
        let mut book = Book::default();
        for (photo, lut, fingerprint) in picks {
            book.record(photo, lut, *fingerprint);
        }
        book
    }

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argentum-look-picks-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 5D Mark II RAWs the rule was set on: As Shot kelvin, scene EV and warmth
    /// after white balance; the hardest of 210.
    #[test]
    fn lamp_light_is_warm_dim_and_stays_warm() {
        let lamps = [
            (3632.0, 6.3, 1.66),
            (3647.0, 9.6, 1.40),
            (3687.0, 4.6, 1.55),
            (3747.0, 5.0, 2.15),
            (3592.0, 6.0, 1.09),
            (3611.0, 10.0, 1.34),
        ];
        let daylight = [
            (6109.0, 12.2, -0.84),
            (4111.0, 10.0, 0.09),
            (4439.0, 13.6, -1.26),
            (4481.0, 7.6, -0.01),
            (3972.0, 10.3, 1.26),
            (4502.0, 15.6, 1.29),
            // Misread as warm light by the camera, on leaves and by a window.
            (3730.0, 6.9, 0.38),
            (3785.0, 7.7, 0.03),
            (4235.0, 6.2, -0.21),
            (3904.0, 9.3, 0.34),
            (4017.0, 8.3, 0.96),
            (4006.0, 8.7, 0.64),
        ];
        for (kelvin, ev, warmth) in lamps {
            let light = light_of(Some(kelvin), Some(ev), warmth);
            assert_eq!(light, Light::Lamp, "{kelvin} K, EV {ev}, warmth {warmth}");
        }
        for (kelvin, ev, warmth) in daylight {
            let light = light_of(Some(kelvin), Some(ev), warmth);
            assert_eq!(
                light,
                Light::Daylight,
                "{kelvin} K, EV {ev}, warmth {warmth}"
            );
        }
        assert_eq!(
            light_of(Some(2850.0), None, 1.8),
            Light::Lamp,
            "tungsten, no EXIF"
        );
        assert_eq!(
            light_of(Some(5500.0), Some(5.0), 0.2),
            Light::Daylight,
            "window light"
        );
        assert_eq!(light_of(None, None, 2.5), Light::Lamp, "a very warm JPEG");
        assert_eq!(light_of(None, Some(5.0), 0.5), Light::Daylight);
    }

    #[test]
    fn the_scene_is_measured_from_its_exposure() {
        let exif = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let sunny = exif(&[
            ("FNumber", "f/16"),
            ("ExposureTime", "1/100 s"),
            ("PhotographicSensitivity", "100"),
        ]);
        assert!((scene_ev100(&sunny).unwrap() - 14.64).abs() < 0.01);
        let lamp = exif(&[
            ("FNumber", "f/2.8"),
            ("ExposureTime", "1/30 s"),
            ("ISOSpeedRatings", "1600"),
        ]);
        assert!((scene_ev100(&lamp).unwrap() - 3.88).abs() < 0.01);
        assert_eq!(scene_ev100(&exif(&[("FNumber", "f/4")])), None);
    }

    #[test]
    fn colours_are_counted_by_hue() {
        let leaves = vec![[0.05, 0.3, 0.04]; 100];
        let sky = vec![[0.2, 0.4, 0.9]; 100];
        let wall = vec![[0.5, 0.5, 0.5]; 100];
        let lamp = vec![[0.8, 0.35, 0.08]; 100];
        assert!(colours(&leaves).green > 0.99);
        assert!(colours(&sky).sky > 0.99);
        assert!(colours(&wall).neutral > 0.99);
        let warm = colours(&lamp);
        assert!(warm.warm > 0.99);
        assert!(warm.warmth_log2 > LAMP_ABOVE_WARMTH_LOG2, "{warm:?}");
        assert_eq!(colours(&[]), Colours::default());
    }

    #[test]
    fn exposure_does_not_change_the_colours() {
        let photo: Vec<[f32; 3]> = (0..200)
            .map(|i| {
                let t = i as f32 / 200.0;
                [0.1 + 0.4 * t, 0.3 * t + 0.05, 0.6 - 0.5 * t]
            })
            .collect();
        let darker: Vec<[f32; 3]> = photo.iter().map(|p| p.map(|v| v / 4.0)).collect();
        let (a, b) = (colours(&photo), colours(&darker));
        assert!((a.green - b.green).abs() < 1e-6 && (a.chroma - b.chroma).abs() < 1e-4);
        assert!((a.key_ev - b.key_ev - 2.0).abs() < 1e-3, "two stops darker");
    }

    #[test]
    fn another_light_is_further_than_a_slightly_different_colour() {
        let leaves = print(Light::Daylight, 0.5, 0.3, 0.0);
        let fewer_leaves = print(Light::Daylight, 0.8, 0.2, 0.0);
        let leaves_under_lamps = print(Light::Lamp, 0.5, 0.3, 0.0);
        assert_eq!(leaves.distance(&leaves), 0.0);
        assert!(leaves.distance(&fewer_leaves) < leaves.distance(&leaves_under_lamps));
    }

    #[test]
    fn nothing_is_suggested_before_three_photos_have_a_pick() {
        let here = print(Light::Lamp, 3.0, 0.0, 0.0);
        let two = book(&[("a", "Portra 800", here), ("b", "Portra 800", here)]);
        assert!(two.suggest("new", &here).is_empty());
        let three = book(&[
            ("a", "Portra 800", here),
            ("b", "Portra 800", here),
            ("c", "Velvia", here),
        ]);
        assert_eq!(three.suggest("new", &here)[0].lut, "Portra 800");
    }

    #[test]
    fn the_nearest_picks_in_the_same_light_vote() {
        let lamp = print(Light::Lamp, 3.0, 0.0, 0.0);
        let leaves = print(Light::Daylight, 0.5, 0.4, 0.0);
        let sky = print(Light::Daylight, -1.0, 0.0, 0.6);
        let picks = book(&[
            ("bed1", "Portra 800", lamp),
            ("bed2", "Portra 800", lamp),
            ("bed3", "500T", lamp),
            ("tree1", "Pro 400H", leaves),
            ("tree2", "Pro 400H", leaves),
            ("roof", "Velvia", sky),
        ]);
        let bed = picks.suggest("bed4", &lamp);
        assert_eq!(
            bed.iter().map(|s| s.lut.as_str()).collect::<Vec<_>>(),
            ["Portra 800", "500T"],
            "only looks picked under lamps",
        );
        assert_eq!((bed[0].votes, bed[0].light), (2, Light::Lamp));
        let tree = picks.suggest("tree3", &leaves);
        assert_eq!(tree[0].lut, "Pro 400H");
        assert_eq!(tree[1].lut, "Velvia", "daylight, further in colour");
    }

    #[test]
    fn with_no_pick_in_its_light_the_nearest_others_are_offered() {
        let day = print(Light::Daylight, 0.0, 0.0, 0.5);
        let picks = book(&[
            ("a", "Velvia", day),
            ("b", "Velvia", day),
            ("c", "Superia", day),
        ]);
        let under_lamps = picks.suggest("bed", &print(Light::Lamp, 3.0, 0.0, 0.0));
        assert_eq!(under_lamps[0].lut, "Velvia");
        assert_eq!((under_lamps[0].votes, under_lamps[0].picks), (0, 2));
        assert_eq!(under_lamps[0].light, Light::Daylight);
    }

    #[test]
    fn a_photo_has_one_pick_and_its_own_never_counts_for_it() {
        let here = print(Light::Lamp, 3.0, 0.0, 0.0);
        let mut picks = book(&[
            ("a", "Portra 800", here),
            ("b", "Portra 800", here),
            ("c", "500T", here),
        ]);
        let before = picks.suggest("c", &here);
        picks.record("c", "Velvia", here);
        picks.record("c", "Gold", here);
        assert_eq!(
            picks.picks.len(),
            3,
            "the last LUT put on is the photo's pick"
        );
        assert_eq!(picks.picks[2].lut, "Gold");
        assert_eq!(
            picks.suggest("c", &here),
            before,
            "trying LUTs on it moves nothing"
        );
        picks.forget("c");
        assert_eq!(picks.picks.len(), 2);
    }

    #[test]
    fn each_pick_keeps_where_its_lut_stood() {
        let here = print(Light::Lamp, 3.0, 0.0, 0.0);
        let mut picks = book(&[
            ("a", "Portra 800", here),
            ("b", "Portra 800", here),
            ("c", "500T", here),
        ]);
        assert_eq!(
            picks.hits(),
            (0, 0),
            "no suggestions while the first three were picked"
        );
        picks.record("d", "Portra 800", here);
        picks.record("e", "Velvia", here);
        assert_eq!(picks.picks[3].rank, Some(1));
        assert_eq!(picks.picks[4].rank, Some(0));
        assert_eq!(picks.hits(), (1, 2));
    }

    #[test]
    fn learning_off_records_and_suggests_nothing() {
        let here = print(Light::Lamp, 3.0, 0.0, 0.0);
        let mut picks = book(&[("a", "P", here), ("b", "P", here), ("c", "P", here)]);
        picks.learning = false;
        picks.record("d", "P", here);
        assert_eq!(picks.picks.len(), 3);
        assert!(picks.suggest("e", &here).is_empty());
    }

    #[test]
    fn the_picks_survive_the_file_and_a_bad_file_starts_empty() {
        let dir = scratch("file");
        assert_eq!(load(&dir), Book::default());
        let here = print(Light::Daylight, 0.2, 0.3, 0.1);
        change(&dir, |book| book.record("a", "Velvia", here)).expect("write");
        let read = load(&dir);
        assert_eq!(read.picks.len(), 1);
        assert_eq!(read.picks[0].fingerprint, here);
        assert!(!dir.join(format!("{FILE}.tmp")).exists());

        std::fs::write(path(&dir), "not json").expect("write");
        assert_eq!(load(&dir), Book::default());
        change(&dir, |book| book.learning = false).expect("write over it");
        assert!(!load(&dir).learning);
    }
}

/// The fingerprints of real photos, to check by eye that the light and the
/// colours read as they should: `AG_RAWS` (paths separated by ';'),
/// `cargo test --lib look_picks::real -- --ignored --nocapture`.
#[cfg(test)]
mod real {
    #[test]
    #[ignore = "reads RAW files; run by hand"]
    fn fingerprints() {
        let raws = std::env::var("AG_RAWS").expect("set AG_RAWS");
        for path in raws.split(';').filter(|p| !p.trim().is_empty()) {
            let bytes = std::fs::read(path).expect("read raw");
            let as_shot = crate::raw_processing::read_as_shot(&bytes)
                .unwrap_or_else(crate::as_shot::AsShot::reference);
            let image = crate::raw_processing::develop_raw_image(
                &bytes,
                false,
                2.5,
                "default".to_string(),
                None,
                Some(path),
            )
            .expect("decodes");
            let loaded = crate::app_state::LoadedImage {
                path: path.to_string(),
                image: std::sync::Arc::new(image),
                is_raw: true,
                as_shot,
            };
            let exif = crate::exif_processing::read_exif_data_from_bytes(path, &bytes);
            let ev = super::scene_ev100(&exif);
            let started = std::time::Instant::now();
            let print = super::measure(&loaded, ev);
            eprintln!(
                "{path}\t{:.0} K\tEV {ev:.1?}\t{:?}\t{:?}\t{:?}",
                as_shot.white_balance.temperature,
                print.light,
                print.colours,
                started.elapsed()
            );
        }
    }
}
