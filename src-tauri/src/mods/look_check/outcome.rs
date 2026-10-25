//! What one case came to against its reference: within the limits or not,
//! and how far it moved since its blessing and since its anchor.

use super::decode_tier;
use super::gate::{self, Drift, Limits};
use super::reference::{self, Case};
use super::stats::{Look, Thumb};

pub struct Outcome {
    /// `sample/state`.
    pub case: String,
    /// The values now, as the case's lines, and a render's thumbnail.
    pub values: Vec<(String, String)>,
    pub thumb: Option<Thumb>,
    /// What it would be blessed on: the adapter for a render, "-" for the
    /// decode, which has none.
    pub adapter: String,
    /// The reference's case, if there is one.
    pub reference: Option<Case>,
    /// Beyond the limits, one line each; empty when within them.
    pub breaches: Vec<String>,
    /// For the summary.
    pub since_blessing: String,
    pub since_anchor: String,
    pub drift: Option<Drift>,
}

impl Outcome {
    fn new(case: String, values: Vec<(String, String)>, adapter: &str, reference: Option<&Case>) -> Self {
        Outcome {
            case,
            values,
            thumb: None,
            adapter: adapter.to_string(),
            reference: reference.cloned(),
            breaches: if reference.is_some() {
                Vec::new()
            } else {
                vec!["no reference yet".into()]
            },
            since_blessing: "new".into(),
            since_anchor: "-".into(),
            drift: None,
        }
    }
}

pub fn of_render(
    case: String,
    look: &Look,
    adapter: &str,
    reference: Option<&Case>,
    limits: &Limits,
) -> Outcome {
    let mut out = Outcome::new(case, reference::stats_lines(&look.stats), adapter, reference);
    out.thumb = Some(look.thumb.clone());
    let Some(r) = reference else {
        return out;
    };
    match reference::look_of(r) {
        Ok(then) => {
            let d = gate::drift(&then, look);
            out.breaches = d.breaches(limits);
            out.since_blessing = d.line();
            out.drift = Some(d);
        }
        Err(e) => out.breaches.push(format!("unreadable reference: {e}")),
    }
    if let Ok(anchor) = reference::stats_from(&r.anchor) {
        let a = gate::stats_drift(&anchor, &look.stats);
        out.since_anchor = format!(
            "median {:.4} EV, p99.9 {:.4} EV, clipped {:.3}%, top bin {:.3}%, blown C* {:+.2}",
            a.midtone_ev,
            a.p999_ev,
            a.clipped * 100.0,
            a.top_bin * 100.0,
            look.stats.blown_chroma - anchor.blown_chroma
        );
    }
    out
}

pub fn of_decode(case: String, values: Vec<(String, String)>, reference: Option<&Case>) -> Outcome {
    let mut out = Outcome::new(case, values, "-", reference);
    if let Some(r) = reference {
        let describe = |lines: Vec<String>| {
            if lines.is_empty() {
                "unchanged".to_string()
            } else {
                lines.join("; ")
            }
        };
        out.breaches = decode_tier::drift(&r.values, &out.values);
        out.since_blessing = describe(out.breaches.clone());
        out.since_anchor = describe(decode_tier::drift(&r.anchor, &out.values));
    }
    out
}
