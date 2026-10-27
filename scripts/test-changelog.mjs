// Tests for scripts/changelog.mjs: run with `node scripts/test-changelog.mjs`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { fold, readFragments, sectionOf } from './changelog.mjs';

const CHANGELOG = `# Changelog

Newest first.

## 26.44.2 — 2026-10-27

### Changed

- **One change.** Already here.

### Fixed

- **One fix.** Already here.
  Second line of it.

## 26.44.1 — 2026-10-27

### Fixed

- **An old fix.**
`;

test('a fragment is named by its section', () => {
  assert.equal(sectionOf('atomic-write.fixed.md'), 'Fixed');
  assert.equal(sectionOf('lazy-locales.removed.md'), 'Removed');
  assert.equal(sectionOf('atomic-write.md'), null);
  assert.equal(sectionOf('atomic-write.bugfix.md'), null);
});

test('fragments land at the end of their section in the top version, as if written there', () => {
  const folded = fold(CHANGELOG, [
    { name: 'b-thumbnails.fixed.md', text: '- **Second fix.**\n' },
    { name: 'a-saves.fixed.md', text: '- **First fix.** Two\n  lines.\n' },
    { name: 'packs.added.md', text: '- **New thing.**\n' },
    { name: 'splash.removed.md', text: '- **Gone.**\n' },
  ]);
  assert.equal(
    folded,
    `# Changelog

Newest first.

## 26.44.2 — 2026-10-27

### Added

- **New thing.**

### Changed

- **One change.** Already here.

### Fixed

- **One fix.** Already here.
  Second line of it.
- **First fix.** Two
  lines.
- **Second fix.**

### Removed

- **Gone.**

## 26.44.1 — 2026-10-27

### Fixed

- **An old fix.**
`,
  );
});

test('no fragments leave CHANGELOG.md as it is', () => {
  assert.equal(fold(CHANGELOG, []), CHANGELOG);
});

test('a fragment with no known section is refused', () => {
  assert.throws(() => fold(CHANGELOG, [{ name: 'oops.md', text: '- x\n' }]), /<topic>\./);
});

test("every fragment in this tree's changelog.d is named right", () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  for (const { name } of readFragments(root)) {
    assert.ok(sectionOf(name), `changelog.d/${name}: name it <topic>.<section>.md`);
  }
});
