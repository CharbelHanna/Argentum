//! Every AI model Argentum can download, in one list. Ours.
//!
//! WHY A LIST OF OUR OWN
//!
//! Each AI feature downloads its own model the first time it is used, into
//! `app_data_dir()/models`, and nothing kept track of what had arrived: a few
//! uses in, that folder holds well over a gigabyte that the user never chose to
//! download and had no way to see or remove. Settings > General > AI Models reads
//! this list to show what is there, download what is not, and delete what is
//! not wanted.
//!
//! ADDING A MODEL IS ONE ENTRY
//!
//! Append a `Model` to `CATALOGUE` below: the files it is saved as (name,
//! address, SHA-256 and size), a label, and one plain-English line saying what
//! it is for. Nothing else changes; the settings page lists whatever is here,
//! downloads exactly these files, and deleting only ever touches names that
//! are here.
//!
//! The feature that uses a model still downloads it itself when it is missing.
//! The settings page's download (`model_download`) fetches the same files from
//! the same addresses and checks them against the same SHA-256, so the feature
//! finds them there and fetches nothing.
//!
//! UPSTREAM'S MODELS ARE COPIES
//!
//! RapidRAW's file names, addresses and checksums are private constants in
//! their `ai_processing.rs`. Making them `pub` would be an edit to their file
//! for the sake of a settings page, so they are copied here instead, and the
//! tests below read their file and fail when the copies drift — a renamed
//! model, or a new one upstream added that this list does not know about yet.
//! That dependency is registered as `model-manager` in
//! `scripts/upstream-registry.mjs`. The selection models are not copied: their
//! list is `rm_selection::manifest`, read through `selection_models::files`.

/// One file of a model: where the feature that uses it saves it, fetches it
/// from, and what it checks it against.
#[derive(Clone, Copy, Debug)]
pub struct File {
    /// Its name in the models folder, or in the entry's own folder.
    pub name: &'static str,
    pub url: &'static str,
    /// Tried when `url` cannot be reached: the same bytes, so the same checksum.
    pub fallback: Option<&'static str>,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// Where a model lives inside the models folder.
#[derive(Clone, Copy, Debug)]
pub enum Stored {
    /// Plain files directly in the models folder. One entry may hold several
    /// files when they only work together.
    Files(&'static [File]),
    /// Files in a folder of their own inside the models folder, listed by the
    /// code that uses them rather than copied here.
    Folder {
        name: &'static str,
        files: fn() -> Vec<File>,
    },
    /// Every folder whose name starts with `prefix`, so a runtime left behind
    /// by an older version is found and removed with the current one. Not
    /// downloaded from the settings page: the feature that needs it installs it.
    FoldersStartingWith {
        prefix: &'static str,
        /// Nothing downloads from here; the tests hold it to the code that does.
        #[cfg_attr(not(test), allow(dead_code))]
        url: &'static str,
        bytes: u64,
    },
}

impl Stored {
    /// The files to download, by name and checksum. None for a runtime folder.
    pub fn files(&self) -> Vec<File> {
        match *self {
            Stored::Files(files) => files.to_vec(),
            Stored::Folder { files, .. } => files(),
            Stored::FoldersStartingWith { .. } => Vec::new(),
        }
    }

    /// The entry's own folder inside the models folder, if it has one.
    pub fn folder(&self) -> Option<&'static str> {
        match *self {
            Stored::Folder { name, .. } => Some(name),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Model {
    /// How the settings page names it when asking for a download or deletion.
    /// Never shown.
    pub id: &'static str,
    /// What the settings page calls it.
    pub label: &'static str,
    /// What it is for, in one plain-English line.
    pub purpose: &'static str,
    pub stored: Stored,
    /// RapidRAW's code downloads it, so `stored` copies constants of theirs,
    /// and the tests check them against their file.
    #[cfg_attr(not(test), allow(dead_code))]
    pub upstream: bool,
    /// Listed even when it is not on disk. Off for things that only exist on
    /// some machines, like the Windows-only graphics card runtime.
    pub listed_when_missing: bool,
}

/// The DirectML runtime's id. `model_manager` asks `gpu_runtime` before
/// removing it: once pinned, it has to stay on disk until the app exits.
pub const GPU_RUNTIME: &str = "gpu-runtime";

/// The selection models' id: `selection_models` downloads under it too.
pub const SELECTION: &str = "selection";

/// One of RapidRAW-Models' files, saved as `$saved` and published as
/// `$published`, written the way their addresses are.
macro_rules! rapidraw {
    ($saved:literal, $published:literal, $sha256:literal, $bytes:expr) => {
        File {
            name: $saved,
            url: concat!(
                "https://huggingface.co/CyberTimon/RapidRAW-Models/resolve/main/",
                $published,
                "?download=true"
            ),
            fallback: None,
            sha256: $sha256,
            bytes: $bytes,
        }
    };
}

pub const CATALOGUE: &[Model] = &[
    // --- RAWmakase's, through `selection_models`: the masks the Masks menu
    // makes since PRs #103 and #104.
    Model {
        id: SELECTION,
        label: "Selection masks",
        purpose: "Finds people, animals and the sky, and picks out what you click, for the Subject, Object, Sky and Background masks.",
        stored: Stored::Folder {
            name: super::selection_models::FOLDER,
            files: super::selection_models::files,
        },
        upstream: false,
        listed_when_missing: true,
    },
    // --- RapidRAW's: their code downloads these four together
    // (`get_or_init_ai_models`) the first time Foreground, Depth, Lens blur,
    // Fog, Quick Erase or Inpainting's Subject is used, so deleting one brings
    // it back with the next of those: all four are in use. Their Subject
    // (SAM 1) and Sky left the Masks menu for the selection masks above; their
    // Sky is still loaded with the others and has to be there for any of
    // them. Loading it only for the Sky mask would save its 168 MB, but that
    // is a change to their loader, not to this list.
    Model {
        id: "subject",
        label: "Subject selection",
        // Also redraws the box of a Subject mask made before the selection
        // masks: the mask is kept in the edit, and only a new box runs this.
        purpose: "Picks out the thing you draw a box around, for Quick Erase and Subject in Inpainting. The Masks menu no longer uses it.",
        stored: Stored::Files(&[
            rapidraw!(
                "sam_vit_b_01ec64_encoder.onnx",
                "sam_vit_b_01ec64_encoder.onnx",
                "16ab73d9c824886f0de2938c19df22fb9ec3deebfd0de58e65177e479213d7d1",
                100_293_382
            ),
            rapidraw!(
                "sam_vit_b_01ec64_decoder.onnx",
                "sam_vit_b_01ec64_decoder.onnx",
                "85d0d672cf5b7fe763edcde429e5533e62f674af4b15c7d688b7673b0ef00bf7",
                8_751_331
            ),
        ]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "foreground",
        label: "Foreground detection",
        purpose: "Finds the main subject on its own, for the Foreground mask.",
        stored: Stored::Files(&[rapidraw!(
            "u2net.onnx",
            "u2net.onnx",
            "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
            175_997_641
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "sky",
        label: "Sky detection",
        purpose: "Loaded with Foreground and Depth, which need it to be here. The Sky mask itself now uses Selection masks.",
        // Saved under an underscore, downloaded from a name with a hyphen.
        stored: Stored::Files(&[rapidraw!(
            "skyseg_u2net.onnx",
            "skyseg-u2net.onnx",
            "ab9c34c64c3d821220a2886a4a06da4642ffa14d5b30e8d5339056a089aa1d39",
            175_997_079
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "depth",
        label: "Depth estimation",
        purpose: "Works out how far away each part of the photo is, for the Depth mask, Lens blur and Fog.",
        stored: Stored::Files(&[rapidraw!(
            "depth_anything_v2_vits.onnx",
            "depth_anything_v2_vits.onnx",
            "d2b11a11c1d4a12b47608fa65a17ee9a4c605b55ee1730c8e3b526304f2562be",
            99_373_606
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    // --- RapidRAW's: one feature each.
    Model {
        id: "relight",
        label: "Surface directions",
        purpose: "Works out which way each surface faces, for Relight.",
        stored: Stored::Files(&[rapidraw!(
            "moge-2-vits-normal-onnx.onnx",
            "moge-2-vits-normal-onnx.onnx",
            "24eacb5dc7a2c54c7bc98f7de085ffbed79ad006ea5b664c2c2cdc02ff3a52f0",
            140_852_051
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "inpainting",
        label: "Inpainting",
        purpose: "Fills in the area you erase with Inpainting.",
        stored: Stored::Files(&[rapidraw!(
            "lama_fp16.onnx",
            "lama_fp16.onnx",
            "2d6be6277c400d6f1b91819737f7c3da935e5c63d1b521d393be1196a2bfa82c",
            111_545_583
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "denoise",
        label: "AI noise reduction",
        purpose: "Cleans up noise when you use Denoise Image.",
        stored: Stored::Files(&[rapidraw!(
            "nind_denoise_utnet_684.onnx",
            "nind_denoise_utnet_684.onnx",
            "ee3586279d514df557ff3f7dec6df37fafc51ba5d3a3435b2cc9ac2d9017e7fe",
            124_202_581
        )]),
        upstream: true,
        listed_when_missing: true,
    },
    Model {
        id: "tagging",
        label: "Photo tagging",
        purpose: "Tags photos by what is in them so a folder can be searched. Only used when AI Tagging is on.",
        stored: Stored::Files(&[
            rapidraw!(
                "clip_model.onnx",
                "clip_model.onnx",
                "57879bb1c23cdeb350d23569dd251ed4b740a96d747c529e94a2bb8040ac5d00",
                605_804_513
            ),
            // Their code checks no checksum for the tokenizer. This one is
            // ours: the file Hugging Face served on 2026-10-21 (git blob
            // 564c0ebd, the one their listing names).
            rapidraw!(
                "clip_tokenizer.json",
                "clip_tokenizer.json",
                "b556ac8c99757ffb677208af34bc8c6721572114111a6e0aaf5fa69ff0b8d842",
                2_224_041
            ),
        ]),
        upstream: true,
        listed_when_missing: true,
    },
    // --- Argentum's.
    Model {
        id: "super-resolution-2x",
        label: "Super Resolution 2x",
        purpose: "Enlarges a photo to twice its size.",
        stored: Stored::Files(&[File {
            name: "realesrgan_x2plus.onnx",
            url: "https://huggingface.co/fernandotonon/QtMeshEditor-realesrgan-onnx/resolve/main/RealESRGAN_x2plus.onnx?download=true",
            fallback: None,
            sha256: "35d016524a6eb7e9a99f192cfa81b6cf1f80fdf8f1b51605d77f0763f5ccba7f",
            bytes: 67_156_218,
        }]),
        upstream: false,
        listed_when_missing: true,
    },
    Model {
        id: "super-resolution-4x",
        label: "Super Resolution 4x",
        purpose: "Enlarges a photo to four times its size.",
        stored: Stored::Files(&[File {
            name: "realesrgan_x4plus.onnx",
            url: "https://huggingface.co/mhmtaufiq/realesrgan-onnx/resolve/main/RealESRGAN_x4plus.onnx?download=true",
            fallback: None,
            sha256: "3767c17388381cfca3d7196a4a517737fefc22b57c9fd98d3bae78e98e2bebc9",
            bytes: 68_811_310,
        }]),
        upstream: false,
        listed_when_missing: true,
    },
    Model {
        id: "restore",
        label: "Restore",
        purpose: "Undoes lens softness, such as a lens used wide open, at the photo's own size. Used by Restore in the Super Resolution window.",
        stored: Stored::Files(&[File {
            name: "omnilens_swinir_aodlib_256.onnx",
            url: "https://github.com/akhater/argentum-models/releases/download/omnilens-v1/omnilens_swinir_aodlib_256.onnx",
            fallback: None,
            sha256: "c28ec5478a6db236e9151961ad1afe29ea23b80fecc8612a436f329e9a0e9630",
            bytes: 53_592_179,
        }]),
        upstream: false,
        listed_when_missing: true,
    },
    Model {
        id: GPU_RUNTIME,
        label: "Graphics card support",
        purpose: "Lets Super Resolution and Restore run on the graphics card, many times faster than without it. Windows only.",
        stored: Stored::FoldersStartingWith {
            prefix: "onnxruntime-directml-",
            url: "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.22.0/microsoft.ml.onnxruntime.directml.1.22.0.nupkg",
            bytes: 17_898_472,
        },
        upstream: false,
        listed_when_missing: false,
    },
];

/// Files an earlier version downloaded for a feature that has since been
/// removed. Nothing lists them, so nothing would ever offer to delete them:
/// `model_manager` removes them at the next start instead.
///
/// - `vitmatte_small_composition_1k.onnx`, about 100 MB: the Object mask's edge
///   model, 26.41.4 only. The Object mask lost to the Subject mask's box.
pub const RETIRED: &[&str] = &["vitmatte_small_composition_1k.onnx"];

/// `(old name, current name)`: a model their code renames to the current
/// name the first time it is used (`promote_legacy_model_filename`). Kept
/// while the current one is missing, since it is that model; a leftover once
/// the current one is there.
pub const RENAMED: &[(&str, &str)] = &[("skyseg-u2net.onnx", "skyseg_u2net.onnx")];

/// The catalogue entry with this id.
pub fn find(id: &str) -> Option<&'static Model> {
    CATALOGUE.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const THEIR_AI: &str = include_str!("../ai_processing.rs");
    const OUR_SUPER_RESOLUTION: &str = include_str!("super_resolution.rs");
    const OUR_GPU_RUNTIME: &str = include_str!("gpu_runtime.rs");

    /// The value of every `const NAME<suffix>: &str = "...";` in their file.
    fn their_constants(suffix: &str) -> Vec<(String, String)> {
        THEIR_AI
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("const ")?;
                let (name, value) = rest.split_once(": &str = \"")?;
                if !name.ends_with(suffix) {
                    return None;
                }
                let value = value.strip_suffix("\";")?;
                Some((name.to_string(), value.to_string()))
            })
            .collect()
    }

    #[test]
    fn ids_are_unique_and_every_entry_names_something() {
        let mut seen = HashSet::new();
        for model in CATALOGUE {
            assert!(seen.insert(model.id), "duplicate id {}", model.id);
            assert!(!model.label.is_empty() && !model.purpose.is_empty());
            match model.stored {
                Stored::FoldersStartingWith { prefix, .. } => {
                    assert!(!prefix.is_empty(), "{} matches every folder", model.id)
                }
                stored => assert!(!stored.files().is_empty(), "{} stores nothing", model.id),
            }
        }
    }

    /// Deleting joins these onto the models folder; a separator or `..` here
    /// would reach outside it. `model_manager` refuses those too, but a list
    /// that never contains one is the first line.
    #[test]
    fn every_name_is_a_bare_file_name() {
        let bare = super::super::model_manager::is_bare_name;
        for model in CATALOGUE {
            let mut names: Vec<&str> = model.stored.files().iter().map(|f| f.name).collect();
            names.extend(model.stored.folder());
            if let Stored::FoldersStartingWith { prefix, .. } = model.stored {
                names.push(prefix);
            }
            for name in names {
                assert!(bare(name), "{} names {name:?}", model.id);
            }
        }
        for (old, current) in RENAMED {
            assert!(bare(old) && bare(current), "{old:?} -> {current:?}");
        }
    }

    /// Every file a download writes is checked against a pinned SHA-256, and
    /// comes from an address that cannot quietly change protocol.
    #[test]
    fn every_file_is_pinned() {
        for model in CATALOGUE {
            for file in model.stored.files() {
                assert_eq!(file.sha256.len(), 64, "{}", file.name);
                assert!(
                    file.sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "{}",
                    file.name
                );
                assert!(file.bytes > 0, "{}", file.name);
                for url in std::iter::once(file.url).chain(file.fallback) {
                    assert!(url.starts_with("https://"), "{url}");
                }
            }
        }
    }

    /// The copies of RapidRAW's constants are still what their code uses. A
    /// rename upstream would otherwise leave a model their code downloads that
    /// this list cannot see, a row here for a file that never arrives, or a
    /// download their code would reject and fetch again.
    #[test]
    fn upstream_names_addresses_and_checksums_match_their_constants() {
        for model in CATALOGUE.iter().filter(|m| m.upstream) {
            for file in model.stored.files() {
                for value in [file.name, file.url] {
                    assert!(
                        THEIR_AI.contains(&format!("\"{value}\"")),
                        "{}: {value} is no longer in ai_processing.rs",
                        model.id
                    );
                }
                // The one file their code never checks.
                if file.name == "clip_tokenizer.json" {
                    assert!(!THEIR_AI.contains(&format!("\"{}\"", file.sha256)));
                    continue;
                }
                assert!(
                    THEIR_AI.contains(&format!("\"{}\"", file.sha256)),
                    "{}: the checksum of {} is no longer the one in ai_processing.rs",
                    model.id,
                    file.name
                );
            }
        }
    }

    /// The other direction: every model file their code saves is in this list.
    /// When upstream adds a model, this fails until it has an entry, so the
    /// settings page never quietly under-reports what is on disk.
    #[test]
    fn every_model_their_code_saves_is_listed() {
        let ours: HashSet<&str> = CATALOGUE
            .iter()
            .filter(|m| m.upstream)
            .flat_map(|m| m.stored.files().into_iter().map(|f| f.name))
            .collect();
        let theirs = their_constants("_FILENAME");
        assert!(
            theirs.len() >= 10,
            "found only {theirs:?}; has the file's layout changed?"
        );
        for (constant, value) in theirs {
            // A name their code renames to the current one on first use.
            if constant.contains("LEGACY") {
                assert!(
                    RENAMED.iter().any(|(old, _)| *old == value),
                    "{constant} = {value:?} is not in RENAMED"
                );
                continue;
            }
            assert!(
                ours.contains(value.as_str()),
                "{constant} = {value:?} in ai_processing.rs has no entry in CATALOGUE"
            );
        }
    }

    /// Each old name is renamed to a name a listed model uses.
    #[test]
    fn renamed_files_become_listed_ones() {
        for (old, current) in RENAMED {
            assert!(
                CATALOGUE
                    .iter()
                    .any(|m| m.stored.files().iter().any(|f| f.name == *current)),
                "{old} becomes {current}, which nothing lists"
            );
        }
    }

    /// A retired name is deleted without asking, so it must be a bare name and
    /// must never be one a listed model still uses.
    #[test]
    fn retired_names_are_bare_and_belong_to_nothing_listed() {
        for name in RETIRED {
            assert!(super::super::model_manager::is_bare_name(name), "{name:?}");
            for model in CATALOGUE {
                let taken = match model.stored {
                    Stored::FoldersStartingWith { prefix, .. } => name.starts_with(prefix),
                    stored => {
                        stored.folder() == Some(*name)
                            || stored.files().iter().any(|f| f.name == *name)
                    }
                };
                assert!(!taken, "{name} is retired and still used by {}", model.id);
            }
        }
    }

    /// Ours are copies too, of `super_resolution` and `gpu_runtime`.
    #[test]
    fn our_names_addresses_and_checksums_match_the_features_that_download_them() {
        for id in ["super-resolution-2x", "super-resolution-4x", "restore"] {
            let model = find(id).expect("listed");
            for file in model.stored.files() {
                for value in [file.name, file.url, file.sha256] {
                    assert!(
                        OUR_SUPER_RESOLUTION.contains(&format!("\"{value}\"")),
                        "{id}: {value}"
                    );
                }
            }
        }
        let Stored::FoldersStartingWith { prefix, url, .. } =
            find(GPU_RUNTIME).expect("listed").stored
        else {
            panic!("the graphics card runtime is a folder");
        };
        assert!(OUR_GPU_RUNTIME.contains(&format!("\"{prefix}")), "{prefix}");
        assert!(OUR_GPU_RUNTIME.contains(&format!("\"{url}\"")), "{url}");
    }

    /// The selection entry is the selection feature's own list, in its folder.
    #[test]
    fn the_selection_entry_is_what_selection_models_reads() {
        let stored = find(SELECTION).expect("listed").stored;
        assert_eq!(
            stored.folder(),
            Some(super::super::selection_models::FOLDER)
        );
        let names: Vec<&str> = stored.files().iter().map(|f| f.name).collect();
        let theirs: Vec<&str> = rm_selection::manifest::all_files()
            .iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, theirs);
        assert_eq!(
            stored.files().iter().map(|f| f.bytes).sum::<u64>(),
            rm_selection::manifest::total_bytes()
        );
    }

    /// Every source file that puts something in the models folder is one this
    /// list was checked against above. A new feature saving a model there
    /// fails this until it has an entry here and a line in `WRITERS`.
    #[test]
    fn every_writer_into_the_models_folder_is_known() {
        // Each, with the test above that holds the catalogue to it.
        const WRITERS: &[&str] = &[
            "ai_processing.rs", // upstream_names_addresses_and_checksums_match_their_constants
            "mods/super_resolution.rs", // our_names_addresses_and_checksums_match_the_features_that_download_them
            "mods/gpu_runtime.rs",      // the same
            "mods/selection_models.rs", // the_selection_entry_is_what_selection_models_reads
            "mods/model_manager.rs",    // the folder itself, for listing and deleting
        ];
        let needle = concat!("join(", "\"models\")");
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && std::fs::read_to_string(&path).unwrap().contains(needle)
                {
                    let rel = path.strip_prefix(&src).unwrap();
                    found.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        found.sort();
        let mut known: Vec<String> = WRITERS.iter().map(|s| s.to_string()).collect();
        known.sort();
        assert_eq!(found, known, "a file writes to the models folder unlisted");
    }
}
