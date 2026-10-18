//! The interface's side of the contract is generated from `src/contract.rs`:
//!
//! - `src/shell/contract.generated.ts`, the TypeScript types;
//! - `src/shell/contract.fixtures.json`, one of each, made by the engine, for
//!   both sides to test against.
//!
//! This test fails when either file is not what `contract.rs` makes now. To
//! write them, in src-tauri/engine:
//!
//!     AG_WRITE_CONTRACT=1 cargo test --test contract

use std::path::PathBuf;

use argentum_engine::contract::{
    Choice, EDIT_VERSION, EditDocument, EditPatch, Event, Opened, Origin, PhotoId, RenderTicket,
    Section, SettingKind, SettingSpec, Stage, ToolDescriptor, ToolEdit, View,
};
use argentum_engine::{Pipeline, standard};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

fn shell() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/shell")
}

fn decl<T: TS>() -> String {
    format!("{}export {}\n", T::DOCS.unwrap_or(""), tidy(&T::decl()))
}

/// ts-rs writes an object type on one line, with its fields' doc comments
/// inline. One field to a line, indented, reads better.
fn tidy(decl: &str) -> String {
    let mut out = String::new();
    // The brackets open at this point. Only an object at the top level has
    // its fields split, and only between fields: `Record<string, X>` keeps
    // its comma.
    let mut open: Vec<char> = Vec::new();
    let mut rest = decl;
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("/**") {
            // A doc comment, copied as it is: its text may hold brackets.
            let end = rest.find("*/").map_or(rest.len(), |i| i + 2);
            out.push_str(&rest[..end].replace('\n', "\n  "));
            rest = &rest[end..];
            continue;
        }
        rest = &rest[c.len_utf8()..];
        let fields = open == ['{'];
        match c {
            '{' if open.is_empty() => {
                open.push(c);
                out.push_str("{\n  ");
                rest = rest.strip_prefix(' ').unwrap_or(rest);
            }
            '}' if fields => {
                open.pop();
                out.truncate(out.trim_end().len());
                out.push_str("\n}");
            }
            ',' if fields => {
                out.push_str(",\n  ");
                rest = rest.strip_prefix(' ').unwrap_or(rest);
            }
            '\n' if fields => out.push_str("\n  "),
            '{' | '<' | '(' | '[' => {
                open.push(c);
                out.push(c);
            }
            '}' | '>' | ')' | ']' => {
                open.pop();
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn typescript() -> String {
    let mut out = String::from(
        "// Generated from src-tauri/engine/src/contract.rs by\n\
         // src-tauri/engine/tests/contract.rs. Do not edit: change contract.rs, then\n\
         // run `AG_WRITE_CONTRACT=1 cargo test --test contract` in src-tauri/engine.\n\
         \n\
         /** A setting's value: a number, a boolean, or the value of a choice. */\n\
         export type SettingValue = number | boolean | string;\n\
         \n",
    );
    out += &format!("export const EDIT_VERSION = {EDIT_VERSION};\n\n");
    for d in [
        decl::<Origin>(),
        decl::<Stage>(),
        decl::<Section>(),
        decl::<SettingKind>(),
        decl::<Choice>(),
        decl::<SettingSpec>(),
        decl::<ToolDescriptor>(),
        decl::<ToolEdit>(),
        decl::<EditDocument>(),
        decl::<EditPatch>(),
        decl::<PhotoId>(),
        decl::<RenderTicket>(),
        decl::<Opened>(),
        decl::<View>(),
        decl::<Event>(),
    ] {
        out += &d;
        out += "\n";
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

/// One of each, made by the engine itself.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Fixtures {
    tools: Vec<ToolDescriptor>,
    edit: EditDocument,
    patch: EditPatch,
    opened: Opened,
    view: View,
    events: Vec<Event>,
}

fn fixtures() -> Fixtures {
    let edit = Pipeline::standard().unwrap().edit();
    let patch: EditPatch = serde_json::from_value(serde_json::json!({
        "ag_exposure_wb": { "exposure": 0.5, "red": 1.05 },
        "rt_capture_sharpen": { "radius": null },
    }))
    .unwrap();
    let mut bins = vec![0u32; 256];
    bins[118] = 1_000_000;
    bins[255] = 500_000;
    Fixtures {
        tools: standard().descriptors(),
        opened: Opened {
            photo_id: PhotoId(1),
            width: 7144,
            height: 4760,
            edit: edit.clone(),
        },
        edit,
        patch,
        view: View {
            zoom: 0.25,
            center_x: 0.5,
            center_y: 0.5,
            width: 1786,
            height: 1190,
        },
        events: vec![
            Event::Frame {
                ticket: RenderTicket(3),
                width: 1786,
                height: 1190,
            },
            Event::Histogram {
                ticket: RenderTicket(3),
                bins,
            },
            Event::Error {
                ticket: Some(RenderTicket(4)),
                message: "no GPU: no adapter".into(),
            },
        ],
    }
}

fn fixtures_json() -> String {
    let mut text = serde_json::to_string_pretty(&fixtures()).unwrap();
    text.push('\n');
    text
}

/// Read with Windows line endings taken out, so a checkout that converts
/// them is not stale.
fn read(path: &PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.replace("\r\n", "\n"))
}

#[test]
fn the_generated_files_are_current() {
    let files = [
        (shell().join("contract.generated.ts"), typescript()),
        (shell().join("contract.fixtures.json"), fixtures_json()),
    ];
    if std::env::var_os("AG_WRITE_CONTRACT").is_some() {
        std::fs::create_dir_all(shell()).unwrap();
        for (path, text) in &files {
            std::fs::write(path, text).unwrap();
            eprintln!("wrote {}", path.display());
        }
        return;
    }
    for (path, text) in &files {
        assert!(
            read(path).as_deref() == Some(text.as_str()),
            "{} is stale: contract.rs now generates something else. \
             Run `AG_WRITE_CONTRACT=1 cargo test --test contract` in src-tauri/engine \
             and commit the result.",
            path.display()
        );
    }
}

#[test]
fn the_fixtures_read_back_as_the_contract() {
    let text = fixtures_json();
    let back: Fixtures = serde_json::from_str(&text).unwrap();
    assert_eq!(back, fixtures());
}

#[test]
fn the_wire_names_are_the_contracts() {
    let event = serde_json::to_value(Event::Frame {
        ticket: RenderTicket(7),
        width: 10,
        height: 20,
    })
    .unwrap();
    assert_eq!(
        event,
        serde_json::json!({ "event": "frame", "ticket": 7, "width": 10, "height": 20 })
    );
    let opened = serde_json::to_value(&fixtures().opened).unwrap();
    assert!(opened.get("photoId").is_some());
    let view = serde_json::to_value(fixtures().view).unwrap();
    assert!(view.get("centerX").is_some());
    let stage = serde_json::to_value(Stage::SceneLinear).unwrap();
    assert_eq!(stage, "scene");
    let origin = serde_json::to_value(Origin::RawTherapee).unwrap();
    assert_eq!(origin, "rt");
}
