//! The run's summary, in Markdown: printed, and appended to `AG_LOOK_SUMMARY`
//! (in CI, the job's summary page). Every case's drift since its blessing and
//! since its anchor, and how many times the ledger has blessed it.

use std::fmt::Write as _;

use super::Run;
use super::gate::Drift;
use super::ledger::{self, Row};

/// A case blessed again this many times within `REBLESS_DAYS` is warned
/// about: blessing the same case over and over is drift with paperwork.
const REBLESS_WARN: usize = 3;
const REBLESS_DAYS: i64 = 30;

/// The summary, and the warnings it raised (also printed for CI to show).
pub fn markdown(run: &Run, rows: &[Row]) -> (String, Vec<String>) {
    let since = (chrono::Local::now() - chrono::Duration::days(REBLESS_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let l = &run.limits;
    let mut md = String::new();
    let mut warnings = Vec::new();
    let _ = writeln!(md, "## Golden photos\n");
    let _ = writeln!(
        md,
        "{} samples, {} renders on {}, `cache_version::PIPELINE` {}, {:.0} s ({}).\n",
        run.samples,
        run.renders,
        run.adapter,
        crate::mods::cache_version::PIPELINE,
        run.seconds,
        run.budget_line
    );
    if let Some(row) = run.blessed_row {
        let _ = writeln!(md, "**Blessed as ledger row {row}.**\n");
    }
    let _ = writeln!(
        md,
        "Limits: cell dE {:.2}, frame dE mean {:.2} / p99 {:.2} / {:.3}% over 2, mid-tones {:.3} EV, clipped {:.3}%, top-stop bin {:.3}%, p99.9 {:.3} EV, blown C* +{:.1}.\n",
        l.cell_de,
        l.frame_de_mean,
        l.frame_de_p99,
        l.frame_de_over_2 * 100.0,
        l.midtone_ev,
        l.clipped * 100.0,
        l.top_bin * 100.0,
        l.p999_ev,
        l.blown_chroma_rise
    );
    md.push_str("| Case | Result | Since its blessing | Since its anchor | Blessed |\n");
    md.push_str("|---|---|---|---|---|\n");
    let mut worst = Drift::default();
    for o in run.outcomes.values().flatten() {
        let result = if o.breaches.is_empty() {
            "ok".to_string()
        } else {
            format!("**moved**: {}", o.breaches.join("; "))
        };
        if let Some(d) = o.drift {
            worst = worst.max(d);
        }
        let again = ledger::reblessings_since(rows, &o.case, &since);
        if again >= REBLESS_WARN {
            warnings.push(format!(
                "{} was blessed again {again} times in {REBLESS_DAYS} days: is it drifting?",
                o.case
            ));
        }
        let row = o.reference.as_ref().map_or("-".into(), |r| {
            format!("row {} ({})", r.blessed.row, r.blessed.date)
        });
        let _ = writeln!(
            md,
            "| {} | {result} | {} | {} | {}x, {row} |",
            o.case,
            o.since_blessing,
            o.since_anchor,
            ledger::blessings(rows, &o.case)
        );
    }
    let _ = writeln!(
        md,
        "\nLargest drift of any case since its blessing: {}.\n",
        worst.line()
    );
    let _ = writeln!(
        md,
        "A one-level highlight shift on each sample's defaults render: caught on {} of {}.",
        run.caught, run.seen
    );
    for note in run.notes.iter().chain(&warnings) {
        let _ = write!(md, "\n- {note}");
    }
    if !run.failures.is_empty() {
        md.push_str("\n\n### Failures\n\n");
        for f in &run.failures {
            let _ = writeln!(md, "- {f}");
        }
    }
    md.push('\n');
    (md, warnings)
}
