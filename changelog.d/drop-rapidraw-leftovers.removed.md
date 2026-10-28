- **Two leftovers of RapidRAW's tree.** ESLint and Prettier no longer skip `src-tauri/rawler`, a
  folder that has not existed since rawler became a Cargo dependency. The Linux `.rpm` no longer
  declares `Provides: rapid-raw`, RapidRAW's own package name, so a package manager cannot take
  Argentum for RapidRAW (`src-tauri/tauri.linux.conf.json`).
