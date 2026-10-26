//! A sample's reference file: one per sample in `src-tauri/tests/look/`,
//! plain text, written by the check and never by hand. Keys and values are
//! separated by a tab:
//!
//! ```text
//! sample 7882
//! camera Canon EOS R5 Mark II
//! sha256 b3c08a5c...
//!
//! [decode]
//! blessed 1 2026-10-25 9 -
//! baseline_ev +0.250
//! ...
//! anchor.baseline_ev +0.250
//!
//! [defaults]
//! blessed 1 2026-10-25 9 llvmpipe (LLVM 19.1.1, 256 bits) (Vulkan, Mesa 25.0.7)
//! luma 0.00121 0.00873 ...
//! ...
//! thumb 32 21
//! 7a37a37a3...   (21 rows of 32 cells, RRRGGGBBB in tenths of an 8-bit level, hex)
//! anchor 2026-10-25
//! anchor.luma 0.00121 0.00873 ...
//! ```
//!
//! `blessed` names the ledger row that blessed the case (LEDGER.tsv), its
//! date, `cache_version::PIPELINE` at the time and the adapter that rendered
//! it. The `anchor` lines are the values of the case's first blessing and are
//! carried over unchanged by every later one.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::stats::{CELL_STEPS, Look, QUANTILES, Stats, TOP_BINS, Thumb};

/// One sample's file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SampleRef {
    pub sample: String,
    pub camera: String,
    pub sha256: String,
    /// By case name: `decode` and the render states.
    pub cases: BTreeMap<String, Case>,
}

/// Who blessed a case, when, and on what.
#[derive(Debug, Clone, PartialEq)]
pub struct Blessing {
    /// The ledger row, counted from 1.
    pub row: usize,
    pub date: String,
    pub pipeline: u32,
    pub adapter: String,
}

/// One case: its blessing, its values in order, and its anchor's.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    pub blessed: Blessing,
    pub values: Vec<(String, String)>,
    /// A render's thumbnail; None for `decode`.
    pub thumb: Option<Thumb>,
    pub anchor_date: String,
    pub anchor: Vec<(String, String)>,
}

impl Case {
    pub fn anchor_value(&self, key: &str) -> Option<&str> {
        self.anchor
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// A thumbnail cell's channel as stored: tenths of an 8-bit level.
fn level_tenths(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * CELL_STEPS).round() as u32
}

pub fn write(sample: &SampleRef) -> String {
    let mut out = String::new();
    out.push_str("# A golden photo's reference, written by src-tauri/src/mods/look_check.rs.\n");
    out.push_str("# Never edited by hand: re-bless with AG_LOOK_BLESS (see that file).\n");
    let _ = writeln!(out, "sample\t{}", sample.sample);
    let _ = writeln!(out, "camera\t{}", sample.camera);
    let _ = writeln!(out, "sha256\t{}", sample.sha256);
    for (name, case) in &sample.cases {
        let b = &case.blessed;
        let _ = writeln!(out, "\n[{name}]");
        let _ = writeln!(
            out,
            "blessed\t{}\t{}\t{}\t{}",
            b.row, b.date, b.pipeline, b.adapter
        );
        for (k, v) in &case.values {
            let _ = writeln!(out, "{k}\t{v}");
        }
        if let Some(thumb) = &case.thumb {
            let _ = writeln!(out, "thumb\t{}\t{}", thumb.width, thumb.height);
            for row in thumb.cells.chunks(thumb.width as usize) {
                for cell in row {
                    for c in cell {
                        let _ = write!(out, "{:03x}", level_tenths(*c));
                    }
                }
                out.push('\n');
            }
        }
        let _ = writeln!(out, "anchor\t{}", case.anchor_date);
        for (k, v) in &case.anchor {
            let _ = writeln!(out, "anchor.{k}\t{v}");
        }
    }
    out
}

pub fn parse(text: &str) -> Result<SampleRef, String> {
    let mut sample = SampleRef::default();
    let mut lines = text.lines().enumerate().peekable();
    let mut current: Option<(String, Case)> = None;
    let finish = |sample: &mut SampleRef, current: &mut Option<(String, Case)>| {
        if let Some((name, case)) = current.take() {
            sample.cases.insert(name, case);
        }
    };
    while let Some((n, line)) = lines.next() {
        let at = || format!("line {}", n + 1);
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            finish(&mut sample, &mut current);
            current = Some((
                name.to_string(),
                Case {
                    blessed: Blessing {
                        row: 0,
                        date: String::new(),
                        pipeline: 0,
                        adapter: String::new(),
                    },
                    values: Vec::new(),
                    thumb: None,
                    anchor_date: String::new(),
                    anchor: Vec::new(),
                },
            ));
            continue;
        }
        let (key, value) = line
            .split_once('\t')
            .ok_or_else(|| format!("{}: no tab in {line:?}", at()))?;
        let Some((_, case)) = current.as_mut() else {
            match key {
                "sample" => sample.sample = value.to_string(),
                "camera" => sample.camera = value.to_string(),
                "sha256" => sample.sha256 = value.to_string(),
                _ => return Err(format!("{}: {key} before any case", at())),
            }
            continue;
        };
        match key {
            "blessed" => {
                let parts: Vec<&str> = value.splitn(4, '\t').collect();
                let [row, date, pipeline, adapter] = parts[..] else {
                    return Err(format!("{}: blessed needs four fields", at()));
                };
                case.blessed = Blessing {
                    row: row.parse().map_err(|_| format!("{}: row {row:?}", at()))?,
                    date: date.to_string(),
                    pipeline: pipeline
                        .parse()
                        .map_err(|_| format!("{}: pipeline {pipeline:?}", at()))?,
                    adapter: adapter.to_string(),
                };
            }
            "thumb" => {
                let (w, h) = value
                    .split_once('\t')
                    .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                    .ok_or_else(|| format!("{}: thumb size {value:?}", at()))?;
                let mut cells = Vec::with_capacity((w * h) as usize);
                for _ in 0..h {
                    let (n, row) = lines
                        .next()
                        .ok_or_else(|| "the thumbnail ends early".to_string())?;
                    if row.len() != w as usize * 9 {
                        return Err(format!("line {}: a thumbnail row of {}", n + 1, row.len()));
                    }
                    for cell in row.as_bytes().chunks(9) {
                        let mut rgb = [0.0f32; 3];
                        for (c, digits) in cell.chunks(3).enumerate() {
                            let text = std::str::from_utf8(digits).map_err(|e| e.to_string())?;
                            let tenths = u32::from_str_radix(text, 16)
                                .map_err(|_| format!("line {}: {text:?}", n + 1))?;
                            rgb[c] = tenths as f32 / CELL_STEPS;
                        }
                        cells.push(rgb);
                    }
                }
                case.thumb = Some(Thumb {
                    width: w,
                    height: h,
                    cells,
                });
            }
            "anchor" => case.anchor_date = value.to_string(),
            _ => match key.strip_prefix("anchor.") {
                Some(k) => case.anchor.push((k.to_string(), value.to_string())),
                None => case.values.push((key.to_string(), value.to_string())),
            },
        }
    }
    finish(&mut sample, &mut current);
    Ok(sample)
}

fn numbers(values: &[f64], digits: usize) -> String {
    values
        .iter()
        .map(|v| format!("{v:.digits$}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A render's statistics as a case's lines, in the order they are written.
pub fn stats_lines(stats: &Stats) -> Vec<(String, String)> {
    let luma: Vec<String> = stats.luma.iter().map(|v| format!("{v:.6e}")).collect();
    vec![
        ("luma".into(), luma.join(" ")),
        ("means".into(), numbers(&stats.means, 6)),
        ("clipped".into(), numbers(&stats.clipped, 7)),
        ("top".into(), numbers(&stats.top, 7)),
        (
            "blown".into(),
            format!("{:.7} {:.3}", stats.blown, stats.blown_chroma),
        ),
    ]
}

/// The statistics back from a case's lines (`values` or `anchor`).
pub fn stats_from(lines: &[(String, String)]) -> Result<Stats, String> {
    let get = |key: &str| -> Result<Vec<f64>, String> {
        let text = lines
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("no {key}"))?;
        text.split(' ')
            .map(|v| v.parse::<f64>().map_err(|_| format!("{key}: {v:?}")))
            .collect()
    };
    fn exact<const N: usize>(key: &str, v: Vec<f64>) -> Result<[f64; N], String> {
        let len = v.len();
        v.try_into()
            .map_err(|_| format!("{key}: {len} values, not {N}"))
    }
    let blown = get("blown")?;
    let [blown, blown_chroma] = exact::<2>("blown", blown)?;
    Ok(Stats {
        luma: exact::<{ QUANTILES.len() }>("luma", get("luma")?)?,
        means: exact::<3>("means", get("means")?)?,
        clipped: exact::<3>("clipped", get("clipped")?)?,
        top: exact::<TOP_BINS>("top", get("top")?)?,
        blown,
        blown_chroma,
    })
}

/// A render case's look back from its file.
pub fn look_of(case: &Case) -> Result<Look, String> {
    Ok(Look {
        stats: stats_from(&case.values)?,
        thumb: case.thumb.clone().ok_or("no thumbnail")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Look {
        Look {
            stats: Stats {
                luma: [0.001, 0.01, 0.05, 0.18, 0.4, 0.8, 0.95, 1.0],
                means: [0.41, 0.4, 0.38],
                clipped: [0.0012, 0.0003, 0.0021],
                top: [0.001; TOP_BINS],
                blown: 0.0025,
                blown_chroma: 3.25,
            },
            thumb: Thumb {
                width: 2,
                height: 1,
                cells: vec![[0.0, 0.5, 1.0], [0.25, 0.75, 0.125]],
            },
        }
    }

    #[test]
    fn a_reference_reads_back_as_written() {
        let look = look();
        let case = Case {
            blessed: Blessing {
                row: 3,
                date: "2026-10-25".into(),
                pipeline: 9,
                adapter: "llvmpipe (Vulkan)".into(),
            },
            values: stats_lines(&look.stats),
            thumb: Some(look.thumb.clone()),
            anchor_date: "2026-10-20".into(),
            anchor: stats_lines(&look.stats),
        };
        let decode = Case {
            values: vec![("baseline_ev".into(), "+0.250".into())],
            thumb: None,
            anchor: vec![("baseline_ev".into(), "+0.300".into())],
            ..case.clone()
        };
        let sample = SampleRef {
            sample: "7882".into(),
            camera: "Canon EOS R5 Mark II".into(),
            sha256: "ab".repeat(32),
            cases: [("defaults".into(), case), ("decode".into(), decode)].into(),
        };
        let text = write(&sample);
        let back = parse(&text).expect("parses");
        assert_eq!(write(&back), text, "written twice, the same text");
        let read = look_of(&back.cases["defaults"]).expect("a look");
        assert_eq!(read.stats, look.stats);
        for (a, b) in read.thumb.cells.iter().zip(&look.thumb.cells) {
            for c in 0..3 {
                assert!((a[c] - b[c]).abs() <= 0.51 / CELL_STEPS, "{a:?} {b:?}");
            }
        }
        assert_eq!(
            back.cases["decode"].anchor_value("baseline_ev"),
            Some("+0.300")
        );
        assert_eq!(back.cases["defaults"].blessed.row, 3);
    }

    /// A render compared with its own reference, kept as text, moves by
    /// nothing: the rounding of what is kept is not a drift.
    #[test]
    fn a_render_against_its_own_reference_moves_by_nothing() {
        let image = image::Rgba32FImage::from_fn(400, 267, |x, y| {
            let t = (x * 7 + y * 3) as f32 / 3600.0;
            image::Rgba([t * 0.9, (t * 1.7).fract(), (1.0 - t).powi(3), 1.0])
        });
        let look = super::super::stats::measure(&image);
        let case = Case {
            blessed: Blessing {
                row: 1,
                date: "2026-10-25".into(),
                pipeline: 9,
                adapter: "-".into(),
            },
            values: stats_lines(&look.stats),
            thumb: Some(look.thumb.clone()),
            anchor_date: "2026-10-25".into(),
            anchor: stats_lines(&look.stats),
        };
        let sample = SampleRef {
            cases: [("defaults".into(), case)].into(),
            ..SampleRef::default()
        };
        let kept = look_of(&parse(&write(&sample)).unwrap().cases["defaults"]).unwrap();
        let d = super::super::gate::drift(&kept, &look);
        assert!(
            d.breaches(&super::super::gate::BOARD).is_empty(),
            "{}",
            d.line()
        );
        assert_eq!(d.cell_de, 0.0, "{}", d.line());
        assert!(d.midtone_ev < 1e-5 && d.p999_ev < 1e-5, "{}", d.line());
    }
}
