//! Our keys in a photo's edit that Copy settings, Paste and presets carry.
//!
//! RapidRAW's paste goes through a saved list of keys
//! (`copyPasteSettings.includedAdjustments`), seeded and extended at each start
//! from their `all_available_adjustments` (app_settings.rs), which chains these
//! in. A key new to that list is switched on for everyone at the next start.
//!
//! The same keys are in their copy groups on the interface's side
//! (`ADJUSTMENT_GROUPS` in utils/adjustments.ts): the camera profile and the
//! RAW rendering as one group, `agSharpen` with Sharpness. Never `rawToneCurve`
//! or `whiteBalanceAuto`: each is worked out for one photo
//! (argentum/profileKeys.ts, argentum/WhiteBalanceMenu.tsx).

pub const KEYS: [&str; 3] = ["cameraProfile", "rawToneRendering", "agSharpen"];

#[cfg(test)]
mod tests {
    use super::KEYS;
    use crate::app_settings::{all_available_adjustments, default_included_adjustments};

    /// Each is offered to paste, and pasted unless the user turns it off.
    #[test]
    fn our_keys_are_pasted_by_default() {
        let available = all_available_adjustments();
        let included = default_included_adjustments();
        for key in KEYS {
            assert!(available.contains(key), "{key} is not offered to paste");
            assert!(included.contains(key), "{key} is not pasted by default");
        }
    }
}
