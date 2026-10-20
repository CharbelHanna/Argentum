//! Our keys in a photo's edit that Copy settings, Paste and presets carry:
//! `cameraProfile`, `rawToneRendering`, `agSharpen`, `whiteBalanceAuto` and
//! `agBlackWhite` (B&W).
//!
//! RapidRAW's paste goes through a saved list of keys
//! (`copyPasteSettings.includedAdjustments`), seeded and extended at each start
//! from their `all_available_adjustments` (app_settings.rs), where ours are
//! written in. A key new to that list is switched on for everyone at the next
//! start. On the interface's side they are in their copy groups
//! (`ADJUSTMENT_GROUPS` in utils/adjustments.ts): the camera profile, the RAW
//! rendering and B&W as one group, `agSharpen` with Sharpness,
//! `whiteBalanceAuto` with White Balance.
//!
//! Auto names the photo it was measured on, and every photo it is pasted onto
//! is measured again for its own light (argentum/autoWhiteBalance.ts,
//! auto_wb_photo.rs). Never `rawToneCurve`: it is fitted to one photo
//! (argentum/RawToneRendering.tsx). A profile that reaches a photo from another
//! camera is left out of its render (profile_correction.rs `fits_this_photo`).
//!
//! What is here is the test that holds their list to ours.

#[cfg(test)]
mod tests {
    use crate::app_settings::{all_available_adjustments, default_included_adjustments};

    /// Each is offered to paste, and pasted unless the user turns it off.
    #[test]
    fn our_keys_are_pasted_by_default() {
        let available = all_available_adjustments();
        let included = default_included_adjustments();
        for key in [
            "cameraProfile",
            "rawToneRendering",
            "agSharpen",
            "whiteBalanceAuto",
            super::super::black_white::KEY,
        ] {
            assert!(available.contains(key), "{key} is not offered to paste");
            assert!(included.contains(key), "{key} is not pasted by default");
        }
    }
}
