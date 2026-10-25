//! The ledger: `src-tauri/tests/look/LEDGER.tsv`, append-only. One row per
//! blessing - its date, its pull request, the cases it was meant to move and
//! one line of why. A reference names the row that blessed it, so every
//! change to how a photo looks can be traced to a reason, and a case blessed
//! again and again shows up.
//!
//! Re-blessing (`AG_LOOK_BLESS`) refuses:
//! - without a reason, or without the cases it means to move (`AG_LOOK_CASES`);
//! - when a case moved that the row does not name;
//! - when the pixels moved and `cache_version::PIPELINE` did not, since every
//!   cached thumbnail would then stay as it was.

use std::path::Path;

/// One row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub date: String,
    pub pr: String,
    /// Case patterns: `7882/defaults`, `7882/*`, `*/portra-400` or `*`.
    pub cases: Vec<String>,
    pub reason: String,
}

const HEADER: &str = "# Append-only: one row per blessing, written by AG_LOOK_BLESS \
                      (src-tauri/src/mods/look_check.rs).\ndate\tpr\tcases\treason\n";

impl Row {
    /// Whether this row names `case` (`sample/state`).
    pub fn names(&self, case: &str) -> bool {
        self.cases.iter().any(|pattern| matches(pattern, case))
    }

    fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.date,
            self.pr,
            self.cases.join(","),
            self.reason.replace(['\t', '\n'], " ")
        )
    }
}

/// `pattern` against `sample/state`, `*` standing for either part or both.
pub fn matches(pattern: &str, case: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let (Some((ps, pc)), Some((s, c))) = (pattern.split_once('/'), case.split_once('/')) else {
        return pattern == case;
    };
    (ps == "*" || ps == s) && (pc == "*" || pc == c)
}

/// The rows, in order; none when the file does not exist yet.
pub fn read(path: &Path) -> Result<Vec<Row>, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .skip(1)
        .map(|line| {
            let fields: Vec<&str> = line.splitn(4, '\t').collect();
            let [date, pr, cases, reason] = fields[..] else {
                return Err(format!("{}: a row without four fields: {line:?}", path.display()));
            };
            Ok(Row {
                date: date.to_string(),
                pr: pr.to_string(),
                cases: cases.split(',').map(|c| c.trim().to_string()).collect(),
                reason: reason.to_string(),
            })
        })
        .collect()
}

/// Add `row` at the end, writing the header first into a new file. Returns
/// the row's number, counted from 1.
pub fn append(path: &Path, row: &Row) -> Result<usize, String> {
    let mut text = std::fs::read_to_string(path).unwrap_or_else(|_| HEADER.to_string());
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&row.line());
    text.push('\n');
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    std::fs::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(read(path)?.len())
}

/// A blessing asked for: `AG_LOOK_BLESS` is the reason, `AG_LOOK_CASES` the
/// cases (comma-separated patterns), `AG_LOOK_PR` the pull request.
#[derive(Debug, Clone, PartialEq)]
pub struct Bless {
    pub reason: String,
    pub cases: Vec<String>,
    pub pr: String,
}

impl Bless {
    /// None when no blessing is asked for; an error when one is asked for
    /// without a reason or without cases.
    pub fn from_env() -> Result<Option<Bless>, String> {
        let Some(reason) = std::env::var_os("AG_LOOK_BLESS") else {
            return Ok(None);
        };
        Bless::new(
            &reason.to_string_lossy(),
            &std::env::var("AG_LOOK_CASES").unwrap_or_default(),
            &std::env::var("AG_LOOK_PR").unwrap_or_default(),
        )
        .map(Some)
    }

    pub fn new(reason: &str, cases: &str, pr: &str) -> Result<Bless, String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(
                "AG_LOOK_BLESS is empty: a blessing needs one line saying why the cases moved"
                    .into(),
            );
        }
        let cases: Vec<String> = cases
            .split(',')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cases.is_empty() {
            return Err(
                "AG_LOOK_CASES is empty: name the cases meant to move (7882/defaults, 7882/*, */portra-400 or *)"
                    .into(),
            );
        }
        let pr = pr.trim();
        Ok(Bless {
            reason: reason.to_string(),
            cases,
            pr: if pr.is_empty() { "-".into() } else { pr.into() },
        })
    }

    pub fn names(&self, case: &str) -> bool {
        self.cases.iter().any(|pattern| matches(pattern, case))
    }
}

/// What a case did since its reference, for the blessing's rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub case: String,
    /// `cache_version::PIPELINE` and the adapter when it was blessed; None
    /// for a case with no reference yet.
    pub blessed: Option<(u32, String)>,
}

/// Whether a blessing may go ahead, given the cases that moved beyond the
/// limits, the pipeline version now and the adapter now.
pub fn may_bless(bless: &Bless, moved: &[Moved], pipeline: u32, adapter: &str) -> Result<(), String> {
    let unnamed: Vec<&str> = moved
        .iter()
        .filter(|m| !bless.names(&m.case))
        .map(|m| m.case.as_str())
        .collect();
    if !unnamed.is_empty() {
        return Err(format!(
            "these cases moved and AG_LOOK_CASES does not name them: {}. Name them if the move is meant, or find what moved them",
            unnamed.join(", ")
        ));
    }
    let stale: Vec<&str> = moved
        .iter()
        .filter(|m| {
            m.blessed
                .as_ref()
                .is_some_and(|(p, a)| *p == pipeline && a == adapter)
        })
        .map(|m| m.case.as_str())
        .collect();
    if !stale.is_empty() {
        return Err(format!(
            "the pixels moved ({}) but cache_version::PIPELINE is still {pipeline}: bump it in src-tauri/src/mods/cache_version.rs, or every cached thumbnail stays as it was",
            stale.join(", ")
        ));
    }
    Ok(())
}

/// How many times `case` was blessed again, after its first blessing, on or
/// after `since` (a `YYYY-MM-DD` date).
pub fn reblessings_since(rows: &[Row], case: &str, since: &str) -> usize {
    rows.iter()
        .filter(|r| r.names(case))
        .skip(1)
        .filter(|r| r.date.as_str() >= since)
        .count()
}

/// How many rows name `case`, its first blessing included.
pub fn blessings(rows: &[Row], case: &str) -> usize {
    rows.iter().filter(|r| r.names(case)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_name_a_sample_a_state_or_everything() {
        assert!(matches("*", "7882/defaults"));
        assert!(matches("7882/*", "7882/decode"));
        assert!(matches("*/portra-400", "8771/portra-400"));
        assert!(matches("7882/defaults", "7882/defaults"));
        assert!(!matches("7882/defaults", "7882/exposure-2"));
        assert!(!matches("8771/*", "7882/defaults"));
    }

    #[test]
    fn a_blessing_without_a_reason_or_cases_is_refused() {
        assert!(Bless::new("  ", "*", "").is_err());
        assert!(Bless::new("Whites from RapidRAW 1.6.7", "", "").is_err());
        let ok = Bless::new("Whites from RapidRAW 1.6.7", "*/highlights-whites", "#171").unwrap();
        assert_eq!(ok.pr, "#171");
    }

    #[test]
    fn a_case_that_moved_unnamed_is_refused() {
        let bless = Bless::new("Whites", "*/highlights-whites", "").unwrap();
        let moved = |case: &str| Moved {
            case: case.into(),
            blessed: Some((9, "llvmpipe".into())),
        };
        assert!(may_bless(&bless, &[moved("7882/highlights-whites")], 10, "llvmpipe").is_ok());
        let err = may_bless(&bless, &[moved("7882/defaults")], 10, "llvmpipe").unwrap_err();
        assert!(err.contains("7882/defaults"), "{err}");
    }

    #[test]
    fn moved_pixels_without_a_new_pipeline_are_refused() {
        let bless = Bless::new("Whites", "*", "").unwrap();
        let moved = [Moved {
            case: "7882/defaults".into(),
            blessed: Some((9, "llvmpipe".into())),
        }];
        assert!(may_bless(&bless, &moved, 9, "llvmpipe").is_err());
        assert!(may_bless(&bless, &moved, 10, "llvmpipe").is_ok());
        // Another adapter is not a change to the pixels.
        assert!(may_bless(&bless, &moved, 9, "WARP").is_ok());
        // Nor is a case seen for the first time.
        let new = [Moved {
            case: "7882/defaults".into(),
            blessed: None,
        }];
        assert!(may_bless(&bless, &new, 9, "llvmpipe").is_ok());
    }

    #[test]
    fn rows_append_and_count() {
        let dir = std::env::temp_dir().join(format!("ag-look-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("LEDGER.tsv");
        let row = |date: &str, cases: &str| Row {
            date: date.into(),
            pr: "-".into(),
            cases: vec![cases.into()],
            reason: "why".into(),
        };
        assert_eq!(append(&path, &row("2026-09-01", "*")).unwrap(), 1);
        assert_eq!(append(&path, &row("2026-10-20", "7882/*")).unwrap(), 2);
        assert_eq!(append(&path, &row("2026-10-21", "7882/defaults")).unwrap(), 3);
        assert_eq!(append(&path, &row("2026-10-22", "*/defaults")).unwrap(), 4);
        let rows = read(&path).unwrap();
        assert_eq!(rows[1], row("2026-10-20", "7882/*"));
        assert_eq!(blessings(&rows, "7882/defaults"), 4);
        assert_eq!(reblessings_since(&rows, "7882/defaults", "2026-09-25"), 3);
        assert_eq!(reblessings_since(&rows, "8771/defaults", "2026-09-25"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
