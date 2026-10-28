// Tests for scripts/installer-size.mjs, on the real sizes of 26.43.4 and 26.44.1.
//
// Run:  npm run test:checks   (or node scripts/test-installer-size.mjs)

import assert from 'node:assert/strict';

import { GROWTH_LIMIT, explained, sizeKey, sizeReport, table } from './installer-size.mjs';

let ran = 0;
let failed = 0;
const test = (name, fn) => {
  ran += 1;
  try {
    fn();
    console.log(`  ok    ${name}`);
  } catch (error) {
    failed += 1;
    console.log(`  FAIL  ${name}`);
    for (const line of String(error.message ?? error).split('\n')) console.log(`          ${line}`);
  }
};

/** Three installers each of 26.43.4 and 26.44.1, with their sizes in bytes, and an updater archive. */
const OLD = [
  { name: '01_Argentum_v26.43.4_windows_x64.exe', size: 23138854 },
  { name: '02_Argentum_v26.43.4_macos-14_aarch64.dmg', size: 33207473 },
  { name: '02_Argentum_v26.43.4_macos-14_aarch64.app.tar.gz', size: 33000000 },
  { name: '03_Argentum_v26.43.4_ubuntu-24.04_amd64.deb', size: 34780098 },
];
const NEW = [
  { name: '01_Argentum_v26.44.1_windows_x64.exe', size: 23687344 },
  { name: '02_Argentum_v26.44.1_macos-14_aarch64.dmg', size: 33635783 },
  { name: '02_Argentum_v26.44.1_macos-14_aarch64.app.tar.gz', size: 99000000 },
  { name: '03_Argentum_v26.44.1_ubuntu-24.04_amd64.deb', size: 35322886 },
];

test('a file is matched across versions', () => {
  assert.equal(sizeKey(OLD[0].name), sizeKey(NEW[0].name));
  assert.notEqual(sizeKey(OLD[0].name), sizeKey(NEW[1].name));
});

test('26.44.1 grew 1.3% to 2.4% over 26.43.4, within the limit, and the updater archive is not counted', () => {
  const { rows, failures } = sizeReport(OLD, NEW, '');
  assert.deepEqual(
    rows.map((r) => r.name),
    [NEW[0].name, NEW[3].name, NEW[1].name],
  );
  assert.ok(rows.every((r) => r.growth > 0.012 && r.growth < 0.025));
  assert.deepEqual(failures, []);
});

test('an installer over the limit fails, unless the notes say why', () => {
  const grown = NEW.map((a, i) => (i === 0 ? { ...a, size: Math.ceil(OLD[0].size * (1 + GROWTH_LIMIT)) + 1 } : a));
  assert.deepEqual(
    sizeReport(OLD, grown, '## Added\n- A thing.').failures.map((r) => r.name),
    [NEW[0].name],
  );
  assert.deepEqual(
    sizeReport(OLD, grown, '## Added\n\n**Installer size:** +0.7 MB, the new face model runtime.').failures,
    [],
  );
});

test('only a line starting "Installer size:" explains', () => {
  assert.ok(explained('Installer size: +1 MB for X'));
  assert.ok(explained('- **Installer size:** +1 MB for X'));
  assert.ok(!explained('The installer size grew.'));
  assert.ok(!explained(undefined));
});

test('the table shows each installer in MB with its change', () => {
  const { rows } = sizeReport(OLD, NEW, '');
  assert.equal(table('26.43.4', rows).split('\n')[2], '| 01_Argentum_v26.44.1_windows_x64.exe | 22.1 | 22.6 | +2.4% |');
});

console.log('');
console.log(`  ${ran - failed}/${ran} passed`);
process.exitCode = failed > 0 ? 1 : 0;
