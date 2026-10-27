- **A pull request no longer re-runs CI because another one merged.** Changelog entries are now
  one file per change in `changelog.d/`, folded into `CHANGELOG.md` at release by
  `scripts/changelog.mjs` (towncrier's shape: the section in the file name), so open PRs stop
  conflicting with each other at the top of `CHANGELOG.md`. And a new push to a PR now cancels
  the run it replaces (`concurrency` in `app.yml`, `engine.yml`, `lint.yml`, `upstream.yml`,
  `dx12.yml`); runs on `main` always finish.
