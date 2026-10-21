# Releasing

A release is a merge commit on `main`, published in this order.

1. **Version.** The tree already carries the next version, bumped right after
   the previous release ("Versioning" in [CHANGELOG.md](../CHANGELOG.md)). If the
   ISO week has rolled over since (`date +%g.%V`), the release sets the real
   `yy.ww.n` in `src-tauri/Cargo.toml`, Argentum's entry in `src-tauri/Cargo.lock`
   and `src-tauri/tauri.conf.json`.
2. **Notes.** The version's section of `CHANGELOG.md`, its plain-language line in
   `src/argentum/releases.ts`, and its rows in `docs/ROADMAP.md` and
   `src/argentum/roadmap.ts` say what actually shipped. Whatever a later PR made
   stale is fixed in a "Release <version>" PR.
3. **Speed check.** At the commit being released, on the laptop the budgets come
   from (named in `src-tauri/src/mods/speed_budgets.rs`), on mains power and with
   nothing else using the GPU, from `src-tauri/`:

   ```bash
   cargo test --lib mods::sharpen_gpu::tests::where_the_time_goes -- --ignored --nocapture --test-threads=1
   ```

   It ends with `speed: every row within its budget`, or fails with one line per
   row over budget: row, size, backend, measured and budget. **A breach blocks
   the release** until the slowdown is fixed. GitHub's runners have no real GPU,
   so this cannot run in CI.
4. **Publish.** `gh release create <version> --target <merge sha> --title
   "Argentum <version> — <summary>" --notes-file <notes>`, with no `v` in the
   tag. Creating the release starts `.github/workflows/release.yml`, which builds
   the installers and attaches them.
5. **Bump.** A "Bump development version to <next>" PR changes the same three
   lines as step 1.
