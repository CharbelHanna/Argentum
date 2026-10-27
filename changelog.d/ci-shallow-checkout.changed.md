- **Release builds check out one commit, not the whole history.** `build.yml` fetched every commit
  (`fetch-depth: 0`) for each of its twelve builds, and nothing in a build reads git: `build.rs`
  does not, and the one dependency that can (`built`, through `rav1e`) is built without git.
