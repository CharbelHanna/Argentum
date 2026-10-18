//! The tools, in the order they run.
//!
//! The list at the bottom is the whole registry: one line per tool, naming
//! its file and its type. That line declares the module and registers the
//! tool, so adding a tool touches this list and nothing else. The pipeline's
//! order comes from here, and so do the interface's panels.

use std::any::TypeId;

use crate::contract::{Origin, ToolDescriptor};
use crate::tool::{AnyTool, Registered, Tool, ToolInfo};
use crate::{Error, Result};

/// An ordered set of tools.
pub struct Registry {
    pub(crate) tools: Vec<Box<dyn AnyTool>>,
    infos: Vec<ToolInfo>,
    descriptors: Vec<std::result::Result<ToolDescriptor, String>>,
    types: Vec<TypeId>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    /// No tools at all.
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            infos: Vec::new(),
            descriptors: Vec::new(),
            types: Vec::new(),
        }
    }

    /// `T` after every tool already listed.
    pub fn with<T: Tool>(mut self) -> Self {
        // An id with no known origin prefix does not compile.
        const {
            assert!(
                Origin::of(T::ID).is_some(),
                "a tool id is dt_, rt_, rr_, rm_ or ag_ and a lowercase name"
            )
        };
        let tool = Registered(T::default());
        self.infos.push(tool.info());
        self.descriptors
            .push(tool.descriptor(self.tools.len() as u32));
        self.types.push(TypeId::of::<T>());
        self.tools.push(Box::new(tool));
        self
    }

    /// Every tool as the interface builds its panel, in pipeline order.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.descriptors
            .iter()
            .filter_map(|d| d.as_ref().ok().cloned())
            .collect()
    }

    pub(crate) fn infos(&self) -> &[ToolInfo] {
        &self.infos
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.infos.iter().position(|t| t.id == id)
    }

    pub(crate) fn position_of<T: Tool>(&self) -> Option<usize> {
        self.types.iter().position(|t| *t == TypeId::of::<T>())
    }

    /// Ids are unique, stages come in order, and every tool's settings can
    /// be described to the interface.
    pub fn check(&self) -> Result<()> {
        for (i, tool) in self.infos.iter().enumerate() {
            if self.infos[..i].iter().any(|t| t.id == tool.id) {
                return Err(Error::Registry(format!("{} is registered twice", tool.id)));
            }
            if let Some(before) = self.infos[..i].iter().find(|t| t.stage > tool.stage) {
                return Err(Error::Registry(format!(
                    "{} runs at {:?} and is listed after {}, which runs at {:?}: \
                     list every Raw tool first, then SceneLinear, then Display",
                    tool.id, tool.stage, before.id, before.stage
                )));
            }
            if let Err(e) = &self.descriptors[i] {
                return Err(Error::Registry(e.clone()));
            }
        }
        Ok(())
    }
}

/// Declares each tool's module and lists the tools, in order.
macro_rules! registry {
    ($($file:literal => $module:ident :: $tool:ident,)*) => {
        $(
            #[path = $file]
            pub mod $module;
        )*

        /// Every tool's module, by its id.
        pub mod tools {
            $(pub use super::$module;)*
        }

        /// The tools Argentum ships, in pipeline order.
        pub fn standard() -> Registry {
            Registry::new()$(.with::<$module::$tool>())*
        }
    };
}

registry! {
    "tools/dt_highlights.rs"      => dt_highlights::DtHighlights,
    "tools/rt_capture_sharpen.rs" => rt_capture_sharpen::RtCaptureSharpen,
    "tools/ag_exposure_wb.rs"     => ag_exposure_wb::AgExposureWb,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Stage;

    #[test]
    fn the_standard_registry_is_consistent() {
        let registry = standard();
        registry.check().expect("valid");
        let ids: Vec<_> = registry.infos().iter().map(|t| t.id).collect();
        assert_eq!(
            ids,
            ["dt_highlights", "rt_capture_sharpen", "ag_exposure_wb"]
        );
        let stages: Vec<_> = registry.infos().iter().map(|t| t.stage).collect();
        assert_eq!(stages, [Stage::Raw, Stage::SceneLinear, Stage::SceneLinear]);
        let orders: Vec<_> = registry.descriptors().iter().map(|d| d.order).collect();
        assert_eq!(orders, [0, 1, 2]);
    }

    #[test]
    fn a_tool_listed_twice_is_refused() {
        let twice = standard().with::<ag_exposure_wb::AgExposureWb>();
        assert!(matches!(twice.check(), Err(Error::Registry(_))));
    }

    #[test]
    fn a_raw_tool_after_a_scene_tool_is_refused() {
        let out_of_order = Registry::new()
            .with::<ag_exposure_wb::AgExposureWb>()
            .with::<dt_highlights::DtHighlights>();
        let err = out_of_order.check().unwrap_err().to_string();
        assert!(err.contains("dt_highlights"), "{err}");
    }
}
