//! Pass test 1: adding a fourth tool is one new file and one registry line.
//!
//! The tool is `support/ag_dummy.rs`. The registry line is the `.with` in
//! `registry()` below - the same call each line of `registry!` in
//! src/registry.rs expands to. Everything else here uses the engine as it
//! is: the tool is listed with what a panel needs, its saved settings are
//! read, migrated and clamped, it runs in order, and its result is cached
//! and invalidated like every other tool's.

#[path = "support/ag_dummy.rs"]
mod ag_dummy;
mod common;

use ag_dummy::AgDummy;
use argentum_engine::tools::{ag_exposure_wb, rt_capture_sharpen};
use argentum_engine::{Kind, Origin, Pipeline, Registry, Size, Stage, standard};

fn registry() -> Registry {
    standard().with::<AgDummy>()
}

fn ran(p: &Pipeline) -> Vec<&'static str> {
    p.last_render()
        .iter()
        .filter(|s| s.ran)
        .map(|s| s.name)
        .collect()
}

fn pipeline() -> Pipeline {
    let mut p = Pipeline::new(registry()).expect("a valid registry");
    // No GPU needed for this test.
    p.set::<rt_capture_sharpen::RtCaptureSharpen>(rt_capture_sharpen::Settings {
        capture: false,
        ..Default::default()
    })
    .unwrap();
    p
}

#[test]
fn a_fourth_tool_is_one_file_and_one_registry_line() {
    let mut p = pipeline();

    // Listed last, with everything a panel needs, read off the tool itself.
    let info = p.registry().tools().last().expect("four tools").clone();
    assert_eq!(p.registry().tools().len(), 4);
    assert_eq!(info.id, "ag_dummy");
    assert_eq!(info.origin, Origin::Argentum);
    assert_eq!(info.stage, Stage::Display);
    assert_eq!(info.kind, Kind::PerPixel);
    assert_eq!(info.version, 2);
    assert_eq!(info.defaults, serde_json::json!({ "amount": 0.0 }));

    // An edit saved by its version 1 is migrated, and clamped to its range.
    p.set_json("ag_dummy", 1, serde_json::json!({ "strength": 50 }))
        .unwrap();
    assert_eq!(p.get::<AgDummy>().unwrap().amount, 0.5);
    p.set_json("ag_dummy", 2, serde_json::json!({ "amount": 7 }))
        .unwrap();
    assert_eq!(p.get::<AgDummy>().unwrap().amount, 1.0);
    assert!(p.set_json("ag_dummy", 3, serde_json::json!({})).is_err());
    p.set_json("ag_dummy", 2, serde_json::json!({ "amount": 0.25 }))
        .unwrap();

    // And saved back at its own version.
    let saved = p.settings_json();
    assert_eq!(
        saved["ag_dummy"],
        serde_json::json!({ "version": 2, "settings": { "amount": 0.25 } })
    );

    // It runs after every other tool, once.
    let source = common::source(256, 192);
    let out = p.render(&source, Size::Full).unwrap();
    assert_eq!(
        ran(&p),
        [
            "dt_highlights",
            "develop",
            "rt_capture_sharpen",
            "ag_exposure_wb",
            "ag_dummy"
        ]
    );

    // Its result is what it does to the tool before it.
    let mut without = pipeline();
    let before = without.render(&source, Size::Full).unwrap();
    let (a, b) = (before.as_rgb().unwrap(), out.as_rgb().unwrap());
    for (x, y) in a.pixels.iter().zip(&b.pixels) {
        for c in 0..3 {
            assert_eq!(y[c], x[c] + (ag_dummy::GREY - x[c]) * 0.25);
        }
    }

    // Cached like the others: nothing runs again, changing it runs it
    // alone, and changing a tool before it runs that tool and it.
    p.render(&source, Size::Full).unwrap();
    assert!(ran(&p).is_empty(), "{:?}", ran(&p));

    p.set_json("ag_dummy", 2, serde_json::json!({ "amount": 0.75 }))
        .unwrap();
    p.render(&source, Size::Full).unwrap();
    assert_eq!(ran(&p), ["ag_dummy"]);

    p.set::<ag_exposure_wb::AgExposureWb>(ag_exposure_wb::Settings {
        exposure: 1.0,
        ..Default::default()
    })
    .unwrap();
    p.render(&source, Size::Full).unwrap();
    assert_eq!(ran(&p), ["ag_exposure_wb", "ag_dummy"]);
    assert_eq!(p.runs("ag_dummy"), 3);
}

#[test]
fn the_standard_registry_does_not_know_it() {
    let p = Pipeline::standard().unwrap();
    assert!(p.registry().position("ag_dummy").is_none());
    assert!(p.get::<AgDummy>().is_none());
}
