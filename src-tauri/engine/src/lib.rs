//! Argentum's engine.
//!
//! Tools from darktable, RawTherapee, RapidRAW, RAWmakase and Argentum, each
//! one file in `src/tools/` and one line in `registry.rs`, run in order by a
//! pipeline that keeps every tool's result. See README.md for the contract
//! and how to add a tool.
//!
//! A library of its own: it builds and tests from its folder without
//! building the app, and the app does not use it yet.

mod develop;
mod error;
mod gpu;
mod image;
pub mod pipeline;
pub mod registry;
pub mod tool;

pub use error::{Error, Result};
pub use gpu::{Gpu, GpuSlot};
pub use image::{Image, Rgb};
pub use pipeline::{Pipeline, Size, Source, Step};
pub use registry::{Registry, standard, tools};
pub use tool::{Context, Kind, Origin, Range, Settings, Stage, Tool, ToolInfo};

/// RawImage and friends, as the tools receive them.
pub use rawler;
