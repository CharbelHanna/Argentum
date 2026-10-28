- **The twelve desktop builds are defined once.** `ci.yml` and `pr-ci.yml` were the same
  manual-only workflow, and `release.yml` held a third copy of their 12-row matrix, so a new
  platform had to be added three times. The two manual copies are gone; `release.yml` also runs
  by hand from the Actions tab, which builds every installer without a release and keeps each
  build's files as an artifact for 7 days. The Download table job runs only for a release.
