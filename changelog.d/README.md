# Changelog entries waiting for a release

A pull request writes its changelog entry here, as its own file, and leaves
`CHANGELOG.md` alone. Every PR adding its entry at the top of `CHANGELOG.md`
put each other open PR in conflict at every merge.

- Name: `<topic>.<section>.md`, the section one of `added`, `changed`,
  `fixed`, `removed`. For example `atomic-write.fixed.md`.
- Content: the bullet(s) exactly as they should read in `CHANGELOG.md`.
- At release, `node scripts/changelog.mjs` folds every file here into the
  version's section of `CHANGELOG.md` and deletes them (`docs/RELEASING.md`).
- `npm run test:checks` fails on a file whose name has no known section.
