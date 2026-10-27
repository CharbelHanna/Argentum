- **The lens makers are asked for once.** Every time Settings opened, its panel asked Rust for the
  lensfun maker list into a list nothing read; My Gear asked again when its section showed, and the
  Crop tab on every visit. The list never changes while the app runs. My Gear and the Crop tab now
  share the first answer (`src/argentum/lensfunMakers.ts`; a failed ask, the database still
  loading, is asked again), and the Settings panel's unread copy is gone.
