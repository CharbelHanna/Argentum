//! What the library shows about a photo's edit. Ours.
//!
//! Worked out here for every way a photo reaches the library: a folder listed,
//! a sidecar read later, a new thumbnail. The field names are the ones the
//! interface reads (`ImageFile` in AppProperties.tsx).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::image_processing::is_image_edited;
use crate::mods::provider_look;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
pub struct EditStatus {
    pub is_edited: bool,
    pub has_provider_look: bool,
}

impl EditStatus {
    pub fn of(adjustments: &Value, is_raw: bool, tonemapper_override: Option<u32>) -> Self {
        Self {
            is_edited: is_image_edited(adjustments, is_raw, tonemapper_override),
            has_provider_look: provider_look::has_look(adjustments),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status(adjustments: Value) -> EditStatus {
        EditStatus::of(&adjustments, true, None)
    }

    fn look() -> Value {
        json!({ "camera": "cam1", "look": "abc" })
    }

    #[test]
    fn a_photo_with_a_look_on_is_edited_and_says_so() {
        assert_eq!(
            status(json!({ "lutPath": provider_look::MARKER, provider_look::KEY: look() })),
            EditStatus {
                is_edited: true,
                has_provider_look: true
            }
        );
    }

    #[test]
    fn any_other_lut_is_only_an_edit() {
        assert_eq!(
            status(json!({ "lutPath": "film.cube" })),
            EditStatus {
                is_edited: true,
                has_provider_look: false
            }
        );
    }

    /// The marker is the switch: a real `lutPath` turns a look off even with
    /// `agProviderLook` still in the edit.
    #[test]
    fn a_look_a_real_lut_replaced_no_longer_counts() {
        let replaced = json!({ "lutPath": "film.cube", provider_look::KEY: look() });
        assert!(!status(replaced).has_provider_look);
    }

    #[test]
    fn a_look_taken_off_no_longer_counts() {
        let off = json!({ "lutPath": provider_look::MARKER, provider_look::KEY: null });
        assert!(!status(off).has_provider_look);
    }

    #[test]
    fn an_empty_edit_says_nothing() {
        assert_eq!(status(json!({})), EditStatus::default());
    }

    /// `#[serde(flatten)]` in `ImageFile` and the interface's field names.
    #[test]
    fn the_interface_reads_these_names() {
        assert_eq!(
            serde_json::to_value(EditStatus::default()).expect("json"),
            json!({ "is_edited": false, "has_provider_look": false })
        );
    }
}
