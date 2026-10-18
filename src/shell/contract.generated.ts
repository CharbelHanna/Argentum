// Generated from src-tauri/engine/src/contract.rs by
// src-tauri/engine/tests/contract.rs. Do not edit: change contract.rs, then
// run `AG_WRITE_CONTRACT=1 cargo test --test contract` in src-tauri/engine.

/** A setting's value: a number, a boolean, or the value of a choice. */
export type SettingValue = number | boolean | string;

export const EDIT_VERSION = 1;

/**
 * Whose code a tool is: the first two letters of its id.
 */
export type Origin = "dt" | "rt" | "rr" | "rm" | "ag";

/**
 * Where in the pipeline a tool runs: on the sensor data before demosaic,
 * on linear light, or on the picture shaped for a screen.
 */
export type Stage = "raw" | "scene" | "display";

/**
 * The panel a tool is shown in. More can be added.
 */
export type Section = "basic" | "tone" | "color" | "detail" | "lens" | "effects";

/**
 * What kind of control a setting is.
 */
export type SettingKind = "float" | "int" | "bool" | "choice";

export type Choice = {
  value: string,
  /**
   * An i18n key.
   */
  label: string,
};

/**
 * One setting of a tool, as a panel shows it.
 */
export type SettingSpec = {
  /**
   * Its name in the tool's settings.
   */
  key: string,
  /**
   * An i18n key: `tools.<tool id>.<key>`.
   */
  label: string,
  kind: SettingKind,
  default: SettingValue,
  min?: number,
  max?: number,
  step?: number,
  unit?: string,
  choices?: Array<Choice>,
  /**
   * Shown under "More" rather than in the panel's first view.
   */
  advanced: boolean,
};

/**
 * One tool, as the interface builds its panel.
 */
export type ToolDescriptor = {
  /**
   * Stable for ever: origin prefix and a name, e.g. `rt_capture_sharpen`.
   */
  id: string,
  origin: Origin,
  /**
   * An i18n key: `tools.<tool id>.label`.
   */
  label: string,
  section: Section,
  /**
   * The tool's place in the pipeline, and in its section.
   */
  order: number,
  stage: Stage,
  /**
   * The version of its settings.
   */
  version: number,
  settings: Array<SettingSpec>,
};

/**
 * One tool's settings in an edit.
 */
export type ToolEdit = {
  /**
   * The version of the tool's settings these were written at.
   */
  v: number,
  settings: Record<string, SettingValue>,
};

/**
 * Every tool's settings for one photo.
 */
export type EditDocument = {
  /**
   * `EDIT_VERSION`.
   */
  version: number,
  tools: Record<string, ToolEdit>,
};

/**
 * A change to an edit: `{ toolId: { key: value } }`, merged into it. `null`
 * puts a setting back to its default. Only the tools it names, and the
 * tools after them, run again.
 */
export type EditPatch = Record<string, Record<string, SettingValue | null>>;

/**
 * Names an open photo.
 */
export type PhotoId = number;

/**
 * Names one requested render. A newer ticket cancels every older one.
 */
export type RenderTicket = number;

/**
 * What `open` returns.
 */
export type Opened = {
  photoId: PhotoId,
  /**
   * The developed picture's size, in pixels.
   */
  width: number,
  height: number,
  edit: EditDocument,
};

/**
 * The part of the photo on screen.
 */
export type View = {
  /**
   * Screen pixels per photo pixel: 1 is 100%.
   */
  zoom: number,
  /**
   * The point of the photo at the centre of the view, 0..1 across and
   * down.
   */
  centerX: number,
  centerY: number,
  /**
   * The view's size in screen pixels.
   */
  width: number,
  height: number,
};

/**
 * What the engine tells the interface, as it happens.
 */
export type Event = {
  "event": "frame",
  ticket: RenderTicket,
  width: number,
  height: number,
} | {
  "event": "histogram",
  ticket: RenderTicket,
  bins: Array<number>,
} | {
  "event": "error",
  ticket?: RenderTicket,
  message: string,
};
