//! How long an editor render may take: the budgets
//! `sharpen_gpu::tests::where_the_time_goes` holds every row to. Ours.
//!
//! WHERE THE NUMBERS COME FROM
//!
//! One laptop: Intel Core Ultra 9 288V with its Arc 140V graphics, driver
//! 32.0.101.6042, on mains power. Measured on DATE at COMMIT in the test
//! profile (plain `cargo test`, no `--release`): five runs of the test, each
//! row's median, and a budget of the larger of 1.5 x the median and the
//! median + 5 ms, rounded up to a whole ms.
//!
//! A slower GPU fails them, so the test stays `#[ignore]`d and is run by hand
//! on that laptop before every release (docs/RELEASING.md). Anywhere else,
//! `AG_SPEED_BUDGETS=report` prints the rows over budget without failing.
//!
//! NEVER WORSE
//!
//! A row over its budget is a slowdown to fix, not a number to raise. When a
//! change makes a row faster, measure again and lower its budget: the test
//! prints its measurements as lines of this table.

/// What `where_the_time_goes` times, in this order everywhere below.
pub const ROWS: [&str; 10] = [
    "theirs alone",
    "measure first",
    "measure again",
    "sharpening first",
    "sharpening kept",
    "mask view on",
    "mask view off",
    "pan step",
    "two eye masks",
    "whole-frame mask",
];

/// Milliseconds per row of `ROWS`, by backend and photo size.
#[rustfmt::skip]
const BUDGETS_MS: [(&str, [u32; 2], [u32; ROWS.len()]); 0] = [
    //                            their  measure    sharpening   mask view  pan   masks:
    //                            alone  1st again  1st   kept   on   off   step  eyes frame
];

/// One line for each row over its budget, or with no budget at all.
pub fn breaches(backend: &str, size: [u32; 2], measured_ms: &[f32; ROWS.len()]) -> Vec<String> {
    let [w, h] = size;
    let Some((.., budget_ms)) = BUDGETS_MS
        .iter()
        .find(|(b, s, _)| *b == backend && *s == size)
    else {
        return vec![format!("{backend} {w}x{h}: no budgets")];
    };
    ROWS.iter()
        .zip(measured_ms.iter().zip(budget_ms))
        .filter(|(_, (ms, budget))| **ms > **budget as f32)
        .map(|(row, (ms, budget))| {
            format!("{backend} {w}x{h} {row}: {ms:.0} ms, budget {budget} ms")
        })
        .collect()
}

/// A table line of what was measured, to lower budgets from.
pub fn table_line(backend: &str, size: [u32; 2], measured_ms: &[f32; ROWS.len()]) -> String {
    let ms: Vec<String> = measured_ms
        .iter()
        .map(|ms| format!("{:4.0}", ms.ceil()))
        .collect();
    format!(
        "{:<14}{size:?}, [{}]),",
        format!("(\"{backend}\","),
        ms.join(",")
    )
}

/// Fail on any breach, unless `AG_SPEED_BUDGETS=report` says these budgets
/// are not this machine's.
pub fn assert_none(breaches: &[String]) {
    if breaches.is_empty() {
        eprintln!("speed: every row within its budget");
        return;
    }
    let lines = breaches.join("\n");
    if std::env::var("AG_SPEED_BUDGETS").as_deref() == Ok("report") {
        eprintln!("speed, report only:\n{lines}");
    } else {
        panic!("over budget:\n{lines}");
    }
}
