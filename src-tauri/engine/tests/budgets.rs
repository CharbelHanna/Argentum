//! North star 5: budgets, measured. Each is a number set from its first
//! measurement and never to be raised.
//!
//! A test binary of its own, with one test in it: anything else running in
//! the same process shares the thread pool and would be timed with it.

mod common;

use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use argentum_engine::Engine;
use argentum_engine::contract::{EditPatch, Event, RenderTicket, View};

/// A slider tick at preview size, end to end through the engine: the patch
/// in, the frame event out. That is the render (only the tool that changed),
/// the view cut out and encoded, and the event.
///
/// Set from its first measurement: 34 MP Canon CR3, Intel Arc 140T,
/// 2026-10-18, a 1600 x 1067 view of the whole photo - median 15 to 18 ms
/// over three runs, with room for a busy machine.
const SLIDER_TICK: Duration = Duration::from_millis(25);

#[test]
fn a_slider_tick_at_preview_size() {
    let Some(path) = common::raw("a_slider_tick_at_preview_size") else {
        return;
    };
    let (tx, rx) = channel();
    let engine = Engine::new(move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    let wait = |ticket: RenderTicket| loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("an event") {
            Event::Frame { ticket: t, .. } if t == ticket => return,
            Event::Error { message, .. } => panic!("{message}"),
            _ => {}
        }
    };

    let opened = engine.open(&path).unwrap();
    let (w, h) = (opened.width as f32, opened.height as f32);
    let view = View {
        zoom: (1600.0 / w).min(1067.0 / h),
        center_x: 0.5,
        center_y: 0.5,
        width: 1600,
        height: 1067,
    };
    let started = Instant::now();
    wait(engine.view(opened.photo_id, view).unwrap());
    eprintln!(
        "\n  {}\n  first preview: {:.0} ms",
        path.display(),
        started.elapsed().as_secs_f64() * 1000.0
    );

    let mut ticks = Vec::new();
    for i in 1..=10 {
        let patch: EditPatch = serde_json::from_value(serde_json::json!({
            "ag_exposure_wb": { "exposure": i as f32 * 0.1 }
        }))
        .unwrap();
        let started = Instant::now();
        wait(engine.apply(opened.photo_id, &patch).unwrap().ticket);
        ticks.push(started.elapsed());
    }
    let ms = |d: &Duration| d.as_secs_f64() * 1000.0;
    eprintln!(
        "  exposure ticks: {} ms",
        ticks
            .iter()
            .map(|t| format!("{:.1}", ms(t)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    ticks.sort();
    // The median, so one tick that waited on the system does not decide it.
    let median = ticks[ticks.len() / 2];
    assert!(
        median <= SLIDER_TICK,
        "a slider tick takes {:.1} ms, the budget is {} ms",
        ms(&median),
        SLIDER_TICK.as_millis()
    );
}
