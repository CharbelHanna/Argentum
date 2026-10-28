- **Release builds: a matrix key nothing read is gone.** The Windows rows of `ci.yml`, `pr-ci.yml`
  and `release.yml` set `builds-args: '--bundles nsis'`, but the build passes `matrix.args`, so
  Windows has always built both installers, `.exe` and `.msi`, as the README lists. The rows now
  say what happens.
