- **The engine's slider-tick budget no longer passes without timing anything.** Without a RAW it
  returned early and reported `ok`, which is what it did on every CI run. It is now `#[ignore]`d,
  so CI reports it as not run, and run by hand it fails when no RAW is set. It joins the app's
  speed check in the release steps (`docs/RELEASING.md`): CI's software GPU cannot hold a budget
  set on a real one.
