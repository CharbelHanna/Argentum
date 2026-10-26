// Tests for scripts/rawmakase-camera-names.mjs.
//
// The one that matters: the committed table is exactly what the script makes
// from the RAWmakase commit it names, byte for byte. A hand edit to a row, or a
// script change that would alter one, fails here. It reads the names from
// GitHub; offline it is skipped, in CI it fails.
//
// Run:  npm run test:checks   (or node scripts/test-rawmakase-camera-names.mjs)

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  TABLE,
  fetchNames,
  parseNames,
  recordedSource,
  regenerate,
  sourceLine,
  tableRows,
} from './rawmakase-camera-names.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const inCI = process.env.CI === 'true' || process.env.CI === '1';

let ran = 0;
let failed = 0;
const test = async (name, fn) => {
  ran += 1;
  try {
    const skipped = await fn();
    console.log(`  ${skipped ? `skip  ${name} (${skipped})` : `ok    ${name}`}`);
  } catch (error) {
    failed += 1;
    console.log(`  FAIL  ${name}`);
    for (const line of String(error.message ?? error).split('\n')) console.log(`          ${line}`);
  }
};

const toml = [
  '# a comment',
  '',
  '[[make]]',
  'libraw = "OM Digital"',
  'adobe = "OM Digital Solutions"',
  '',
  '[[camera]]',
  'make = "Panasonic"',
  'model = "DC-G97"',
  'adobe = "Panasonic DC-G99"',
  'why = "same sensor"',
].join('\n');

await test("a make row and a camera row, each field as written, in the file's order", () => {
  assert.deepEqual(tableRows(parseNames(toml)), [
    'make\tOM Digital\t-\tOM Digital Solutions\t',
    'camera\tPanasonic\tDC-G97\tPanasonic DC-G99\tsame sensor',
  ]);
});

await test('a line that is not a flat string, or a row without its names, stops the script', () => {
  assert.throws(() => parseNames('[[camera]]\nmodel = 3'), /not a key = "string" line/);
  assert.throws(() => parseNames('[[lens]]\nmake = "X"'), /not a key = "string" line/);
  assert.throws(() => parseNames('[[camera]]\nmake = "Panasonic"\nadobe = "X"'), /has no model/);
});

await test('the Source line names the commit, release and counts, and is read back', () => {
  const sha = 'a'.repeat(40);
  const line = sourceLine({ sha, tag: 'v1.2.3' }, tableRows(parseNames(toml)));
  assert.match(line, /@ a{40} \(v1\.2\.3\), 1 make, 1 cameras$/);
  assert.deepEqual(recordedSource(line), { sha, tag: 'v1.2.3' });
  assert.deepEqual(recordedSource(sourceLine({ sha, tag: null }, [])), { sha, tag: null });
});

await test(`${TABLE} is what the script makes from the RAWmakase commit it names`, async () => {
  const tsv = readFileSync(join(root, TABLE), 'utf8');
  const source = recordedSource(tsv);
  assert.ok(source, 'the table has no "# Source:" line');
  let names;
  try {
    names = await fetchNames(source.sha);
  } catch (error) {
    if (inCI) throw error;
    return `could not fetch RAWmakase's names: ${error.cause?.code ?? error.message}`;
  }
  const made = regenerate(tsv, names, source);
  if (made !== tsv) {
    const [a, b] = [tsv.split('\n'), made.split('\n')];
    let i = 0;
    while (a[i] === b[i]) i += 1;
    assert.fail(
      `line ${i + 1} differs - regenerate it, never edit it:\n  table:  ${a[i]?.slice(0, 80)}\n  script: ${b[i]?.slice(0, 80)}`,
    );
  }
});

console.log('');
console.log(`  ${ran - failed}/${ran} passed`);
process.exitCode = failed > 0 ? 1 : 0;
