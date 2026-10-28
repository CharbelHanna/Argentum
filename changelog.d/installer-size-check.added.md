- **A release checks its installers' size.** North star 7 calls installer size the smoke alarm:
  a release that grows it noticeably says why. Nothing checked it. Once every installer is
  attached, the release workflow compares each one (`.exe`, `.msi`, `.dmg`, `.deb`, `.rpm`,
  `.AppImage`) with the same file of the previous release and fails when one grew by more than
  3%, unless the release's notes have a line starting `Installer size:` saying why
  (`scripts/installer-size.mjs`). On real releases, 26.44.1 is within it (+2.4% at most) and
  26.43.1 would have been stopped (+3.8%).
