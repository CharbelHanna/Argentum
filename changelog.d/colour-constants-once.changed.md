- **Colour constants are defined once.** The sRGB curve, the Rec. 709 luminance weights, the
  sRGB-to-XYZ (D65) matrix and both Bradford matrices now live only in the engine's `colour`
  module, and the app imports them; the 3x3 matrix helpers live only in `mods/profile_matrix.rs`.
  Twenty-odd pasted copies are gone. Every copy removed held the same numbers, clamping and linear
  segment as the definition it now uses, so no pixel changes. Two sRGB encoders that are not
  copies stay: `apply_linear_to_srgb` keeps light above white for HDR merges, and the raw tone
  analysis encodes without clipping first.
