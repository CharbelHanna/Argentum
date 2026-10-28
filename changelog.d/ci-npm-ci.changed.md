- **Release builds install the interface's packages from the lock file.** `build.yml` ran
  `npm install`, which may pick newer versions than `package-lock.json` and rewrite it, so a
  release could ship packages no check had seen. It now runs `npm ci`, as the lint job already
  does: exactly the locked versions, or the build stops.
