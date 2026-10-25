// Tests for scripts/check-sources.mjs.
//
// The one that matters: every wiring line is in today's tree, and a tree that
// lost one fails. The rest test the "a harvest review is due" warning without
// the network.
//
// Run:  npm run test:checks   (or node scripts/test-check-sources.mjs)

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { WIRING, harvestWarning, missingWiring, reviewedRelease } from './check-sources.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const readTree = (file) => {
  try {
    return readFileSync(join(root, file), 'utf8');
  } catch {
    return null;
  }
};

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

test('every wiring line is in the tree', () => {
  const missing = missingWiring(readTree);
  assert.deepEqual(
    missing.map(({ file, pattern }) => `${file} ${pattern}`),
    [],
  );
});

test('a file that lost its wiring line is reported', () => {
  const [first] = WIRING;
  const read = (file) => (file === first.file ? readTree(file).replace(first.pattern, '= removed(') : readTree(file));
  assert.deepEqual(
    missingWiring(read).map(({ pattern }) => pattern),
    [first.pattern],
  );
});

test('a file that is gone is reported once per line it held', () => {
  const read = (file) => (file === 'src-tauri/src/formats.rs' ? null : readTree(file));
  assert.equal(missingWiring(read).length, 1);
});

test('the dependency-list pattern is not satisfied by a comment', () => {
  const rule = WIRING.find(({ pattern }) => String(pattern).includes('tiffDepth'));
  assert.equal(rule.pattern.test('// see [tiffDepth] in the effect'), false);
  assert.equal(rule.pattern.test('}, [format, tiffDepth]);'), true);
});

test('the CHANGELOG names the RapidRAW release last read', () => {
  assert.match(reviewedRelease(readTree('CHANGELOG.md')) ?? '', /^v\d+\.\d+\.\d+$/);
  assert.equal(reviewedRelease('Forked from RapidRAW 1.6.6; read up to `v1.6.7`.'), 'v1.6.7');
  assert.equal(reviewedRelease('no marker here'), null);
});

test('a newer RapidRAW release asks for a harvest review', () => {
  const tags = new Map([
    ['v1.6.6', 'a'],
    ['v1.6.7', 'b'],
    ['v1.6.8', 'c'],
  ]);
  assert.match(harvestWarning('v1.6.7', tags), /v1\.6\.8 is out .* read up to v1\.6\.7/);
  assert.equal(harvestWarning('v1.6.8', tags), null);
  assert.match(harvestWarning(null, tags), /names no RapidRAW release/);
});

console.log('');
console.log(`  ${ran - failed}/${ran} passed`);
process.exitCode = failed > 0 ? 1 : 0;
