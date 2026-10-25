# Architecture

Argentum was forked from RapidRAW at 1.6.6. It is an application of its own,
changed one piece at a time, and it keeps working at every step:

> **New code goes in the engine. Everything inherited from RapidRAW is ours to
> change. RapidRAW is a source we harvest from, like darktable, and is never
> merged again.**

---

## Why the fork rule changed

A fork dies when your changes and upstream's land in the same lines of the
same files. Every update becomes an argument, you stop updating, and you are
maintaining a whole photo editor alone. For its first months Argentum answered
that with one rule: our code in our own files, as few lines as possible in
theirs, and every upstream commit that touched something we depend on reviewed
on its own.

By October 2026 the rule no longer held. Our lines were in 66 of RapidRAW's
files, about 2,000 of them, and the review of RapidRAW 1.6.5 recorded 1,242
per-commit decisions, 84% of them "not applicable". The effort went into
saying that unrelated commits were unrelated, while the code that renders a
photo, where Argentum differs most, was the code we were least free to change.

So the line moved: the code that renders a photo and the interface became ours
to change, and only their other files were still merged, once per release.

On 2026-10-25 that ended too. RapidRAW 1.6.7 rewrote its render path, and their
other files (export, files, AI, LUTs, masks) now call it. Over our render path a
merge gave about ninety compile errors, and taking it would have meant porting
some five thousand lines with their pixel changes mixed into the speed work.
Argentum stopped merging RapidRAW for good:

| Where | Rule |
|---|---|
| `src-tauri/engine/` | New tools: one file in `tools/`, one line in `registry.rs` (see its README) |
| Everything else: the files inherited at 1.6.6, `src-tauri/src/mods/`, the shaders, `src/argentum/`, the interface | Ours to change. New code still goes in a module of ours with one job, rather than growing an inherited file |

Inherited code is replaced only when it is dead or measurably worse, and what
nothing uses is deleted, one small change at a time. RapidRAW's later releases
are read and harvested by hand (see "Harvesting from RapidRAW" below).

---

## How that works for shaders

This is how the tools in today's pipeline reach the GPU. New tools go into the
engine instead (`src-tauri/engine/README.md`).

RapidRAW's image engine is one file, `src-tauri/src/shaders/shader.wgsl`.
Every tool is a function; `main()` calls them in order.

Our tools do not go into that file: it is pulled in with Rust's `include_str!`,
and our own shader file is glued in front of it at build time, so each tool of
ours lives in one place.

```rust
// gpu_processing.rs — on the render path, ours
source: wgpu::ShaderSource::Wgsl(
    concat!(
        include_str!("shaders/modules.wgsl"),   // ours
        include_str!("shaders/shader.wgsl"),       // inherited from RapidRAW
    ).into()
)
```

**`modules.wgsl` holds every function we harvest.** The main shader calls it
once per stage, and every tool of ours runs inside a stage (see THE PIPELINE
ANCHOR in `modules.wgsl`):

```wgsl
// in main():
composite_rgb_linear = ag_stage_scene_linear(composite_rgb_linear, /* profile rows */);
```

Those calls are among the wiring `npm run check:sources` guards: if one goes,
a stage's tools stop running without any error.

---

## Layout

```
src-tauri/
  engine/                OURS    — the processing engine: a tool registry, one file per tool
  src/
    shaders/
      shader.wgsl        INHERITED — the main shader, one call per stage into ours
      modules.wgsl       OURS      — harvested math for the current pipeline
    mods/                OURS      — our Rust code, until it moves into the engine
    *.rs                 INHERITED — came from RapidRAW 1.6.6, ours to change

src/
  argentum/              OURS
  App.tsx, components/   INHERITED — the app shell, editor, panels and views
```

**Rule of thumb:** everything is ours to change, and new code goes in a module
of ours with one job. An inherited file gets the smallest change that calls
into it, so the 4,000-line files shrink over time instead of growing.

---

## The color space thing

RapidRAW works in linear sRGB. darktable's modules assume something wider or
more perceptual. Transplanted math needs converting in and back out.

That conversion lives in `modules.wgsl` and is **written once, reused by every
tool after** — not a per-tool cost.

**What actually got built, in `26.37.2`:** linear sRGB ↔ CIE XYZ, plus XYZ ↔
Bradford cone space (LMS). White balance needed a colorimetric space and a cone
space, not a wider RGB one, so that is what existed. (Retired in `26.41.6`,
with our white balance: RapidRAW 1.6.5 does the same in its own engine. The
pattern below stands for the next tool that needs it.)

```wgsl
const AG_SRGB_TO_XYZ  // and AG_XYZ_TO_SRGB
const AG_XYZ_TO_LMS   // and AG_LMS_TO_XYZ  — Bradford
```

```wgsl
fn dt_something(color: vec3<f32>, ...) -> vec3<f32> {
    let xyz = AG_SRGB_TO_XYZ * color;    // in
    // ... darktable's math, largely as-is ...
    return AG_XYZ_TO_SRGB * xyz;         // out
}
```

There is **no `srgb_to_rec2020` and no `mods/color.rs`** — earlier drafts of
this document promised both. If a later harvested module genuinely needs linear
Rec2020, add that matrix pair to `modules.wgsl` alongside the others. Going
through XYZ is the general route; a wide-RGB working space is one destination,
not the only one.

---

## Data and storage

- **Edits** — JSON sidecar next to each photo (`IMG_1234.CR3.agdata`).
  RapidRAW's format, renamed; we add fields to it. Original RAW never modified.
- **Catalog** — SQLite in the data folder. Ours entirely. Lets you browse
  photos whose drive is unplugged.
- **Thumbnails** — JPEGs in the data folder. Already exists upstream; we make
  the location configurable and keep them across sessions.

All of it in one user-chosen folder. See [README](README.md#everything-in-one-folder).

---

## Where a new feature goes

1. **A tool that changes pixels** goes in the engine: one file, one registry line,
   its tests (`src-tauri/engine/README.md`).
2. **Anything else of ours** goes in a module with one job: `src-tauri/src/mods/`
   for Rust, `src/argentum/` for the interface, `shaders/modules.wgsl` for math
   in today's pipeline.
3. **The inherited file** that has to reach it gets the smallest change that calls
   into ours: a call, an import, a mount point. Inherited files are ours to change,
   so this is a choice for clarity, not a rule about someone else's code.

Some of the early features were built without touching an inherited file at all
(a component mounted through a `data-argentum` marker, a store patched at import,
an entry spliced into an exported array). With merging over, that indirection
buys nothing: a direct, readable change is preferred, and the old workarounds are
replaced by direct edits one at a time, each with its test.

---

## Planned: naming the objects before masking them

Written down now because the shape of it is decided and the cost is not where it
looks. Nothing is built yet. See the Masking row in `ROADMAP.md`.

**What it is.** RapidRAW's AI mask (`generate_ai_subject_mask`) takes a
`start_point` and an `end_point`: you drag a box round the thing, SAM segments
inside it. The wanted behaviour is the other way round — look at the photo once,
list what is in it, tick the dog.

**Three models, not one.**

| Step | Model | What it answers |
|---|---|---|
| Name | RAM++ | *what* is in this picture — a list of tags, no positions |
| Locate | Grounding DINO (or YOLO-World) | *where* the thing called "dog" is — a box |
| Cut | SAM | the mask inside that box — **already shipped** |

RAM++ classifies and does not localise. That is the whole reason the middle row
exists, and it is the thing most likely to be misremembered later as "RAM++ gives
you objects". It does not. This chain is Grounded-SAM's own arrangement, so it is
a known-good combination rather than an invention.

**What is already here.** `ort` (ONNX Runtime, `load-dynamic`) is a dependency.
`ai_processing.rs` already downloads a model from a URL into
`get_models_dir(app_handle)`, checks a SHA-256, and caches the session — so two
more models are a list entry, not new machinery. SAM needs a box; we would be
handing it one we computed instead of one the user drew, which is the same
argument it takes today.

**Where the picker goes.** In the Masks tab, which is ours to change. It
*creates* the mask rather than living inside the mask list, so the list keeps one
job.

**Open, and not to be guessed at.** Model licences and sizes (two more downloads
on top of SAM's). Whether the tags are worth storing per photo — they would make
library search work on content, which needs somewhere to put them, and the
catalogue index is the obvious place and does not exist yet.

---

## Harvesting from RapidRAW

RapidRAW is read once per release and nothing is merged. `npm run check:sources`
warns when a release newer than the one named on the CHANGELOG's "read up to"
line is out.

1. **Read the release** from the last one read to the new tag: the release
   notes, the commit list, and their shader changes first, the most valuable
   and the easiest to copy.
2. **One line per feature** with a verdict: **copy** (into a file of ours, with
   the source commit in a comment), **skip** (not wanted: cloud, sync, their
   library), or **decide** (they built something we have).
3. **Measure pixel changes.** A change to how photos look is rendered on a set
   of test RAWs against today's render and compared per pixel (CIEDE2000); the
   numbers decide.
4. **Each copy is its own pull request,** with its tests and a CHANGELOG entry
   naming the source file and commit. Then the "read up to" line moves.

Three lessons from the merging years still hold:

- **They may build what we built.** When a release changes a feature we also
  have, say so and decide between the two on evidence. No diff detects it.
- **Review the interface as well as the processing.** Keeping our processing
  does not mean rejecting a better control or workflow of theirs.
- **A paired change comes over whole.** When their change removes something and
  adds its replacement elsewhere (a new event shape, a renamed setting), copy
  both halves or neither.

---

## Where code comes from

Nothing in this design is darktable-specific. `modules.wgsl` holds *harvested*
math — the source doesn't matter, only that it's a formula we can express as a
shader function.

Name functions by where they came from, so it stays obvious a year later:

| Prefix | Source | Good for |
|---|---|---|
| `dt_` | darktable | color science, tone mapping, denoise, highlight recovery |
| `rt_` | RawTherapee | demosaic, highlight recovery — often the strongest here |
| `gimp_` | GIMP / GEGL | pixel-level effects, blend modes, distortions |
| `gmic_` | G'MIC | huge filter library, film looks, stylisation |
| `paper_` | a published paper | when nobody's implemented it yet |

Honest note on GIMP: it's a *pixel editor*, not a RAW developer, so it has less
to offer at the RAW end than darktable does. Its useful parts are GEGL
operations — self-contained image effects, which is exactly the shape we want.
Worth raiding for creative effects, not for color science.

For anything RAW-specific that darktable doesn't win outright, look at
**RawTherapee** before GIMP. Its highlight recovery and demosaic are
best-in-class, and it's clean C++.

**Same recipe regardless of source.** See [ADDING_A_TOOL.md](ADDING_A_TOOL.md).

---

## Where things live

Everything lives in the checkout: source, `node_modules`, Rust's build output
and the app's data. Git never tracks a binary, so pushing is the only backup
the source needs.

**Keep the checkout out of cloud-synced folders.** A sync client such as
OneDrive does not tolerate directory junctions inside a synced folder: it
replaced a `node_modules` junction with a real directory (reparse tag
`0x9000e01a`, its own cloud-files tag), began syncing 182 npm packages
(239MB), and held them locked while uploading, so they could not be removed
until the sync client was stopped. A bare repository in a synced folder works
as a backup remote; it needs `receive.shallowUpdate true`, because this repo
is shallow at RapidRAW's root.

**Leave Rust's `target/` where Tauri puts it.** A `.cargo/config.toml` once
redirected `target-dir`, and the redirect put `target/` at the repository
root. Vite watches the project directory and only ignores `**/src-tauri/**`,
so it tried to watch 819 crates worth of build artifacts, hit a build script
`.exe` that cargo had open, and crashed with `EBUSY` - which killed the whole
dev command. Tauri's default `src-tauri/target` is already inside Vite's
ignore list and already covered by `.gitignore`.

Default tool behaviour is correct; adding a redirect back is how both of these
breakages happened. `.gitignore` is the safety net: if you add a dependency
that writes something large, make sure it lands in an ignored folder before
committing.
