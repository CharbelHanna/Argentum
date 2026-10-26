// Tests for scripts/rawmakase-lens-baselines.mjs.
//
// The one that matters: the committed table is exactly what the script makes
// from the RAWmakase commit it names, byte for byte. A hand edit to a row, or a
// script change that would alter one, fails here. It reads RAWmakase's record
// from GitHub; offline it is skipped, in CI it fails.
//
// Run:  npm run test:checks   (or node scripts/test-rawmakase-lens-baselines.mjs)

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  SAMPLES,
  TABLE,
  fetchBaselines,
  goldenPhotos,
  lensRows,
  recordedSource,
  regenerate,
  sourceLine,
} from './rawmakase-lens-baselines.mjs';

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

const radial = { values: [1, 1.01], knots: [0, 1] };
const record = {
  files: [
    {
      sha256: 'aaa',
      header: {
        metadata: {
          lens: { source: 'X built-in', default_on: true, distortion: radial, centre: [0.5, 0.5] },
          lens_stand_in: { source: 'X built-in', vignetting: radial, centre: [0.5, 0.5] },
        },
      },
    },
    { sha256: 'bbb', header: { metadata: {} } },
    { sha256: 'ccc', header: { metadata: { lens: { source: 'DNG opcodes', gain_map: {}, distortion: null } } } },
  ],
};
const leftOut = [
  { field: 'centre', reason: 'test', removedWhen: 'test' },
  { field: 'gain_map', reason: 'test', removedWhen: 'test' },
];

await test('a row is a golden photo, its sha256, its record and its stand-in, fields left out, keys in order', () => {
  const rows = lensRows(
    record,
    [
      ['1', 'aaa'],
      ['2', 'bbb'],
      ['3', 'zzz'],
      ['4', 'ccc'],
    ],
    leftOut,
  );
  assert.deepEqual(rows, [
    '1\taaa\t{"default_on":true,"distortion":{"knots":[0,1],"values":[1,1.01]},"source":"X built-in"}' +
      '\t{"source":"X built-in","vignetting":{"knots":[0,1],"values":[1,1.01]}}',
    '2\tbbb\tnull\tnull',
    '4\tccc\t{"distortion":null,"source":"DNG opcodes"}\tnull',
  ]);
});

await test('a field left out that no record holds any more stops the script', () => {
  const stale = [...leftOut, { field: 'gone', reason: 'test', removedWhen: 'test' }];
  assert.throws(
    () =>
      lensRows(
        record,
        [
          ['1', 'aaa'],
          ['4', 'ccc'],
        ],
        stale,
      ),
    /gone - delete them/,
  );
});

await test('the Source line names the commit, release, counts and what is left out, and is read back', () => {
  const sha = 'a'.repeat(40);
  const rows = lensRows(
    record,
    [
      ['1', 'aaa'],
      ['2', 'bbb'],
      ['4', 'ccc'],
    ],
    leftOut,
  );
  const line = sourceLine({ sha, tag: 'v1.2.3' }, rows, leftOut);
  assert.match(line, /3 golden photos, 2 with a lens correction, left out: centre, gain_map$/);
  assert.deepEqual(recordedSource(line), { sha, tag: 'v1.2.3' });
  assert.deepEqual(recordedSource(sourceLine({ sha, tag: null }, rows, leftOut)), { sha, tag: null });
});

await test('the golden photos are read from samples.tsv', () => {
  const photos = goldenPhotos(readFileSync(join(root, SAMPLES), 'utf8'));
  assert.ok(photos.length > 0);
  for (const [id, sha] of photos) assert.match(`${id} ${sha}`, /^\d+ [0-9a-f]{64}$/);
});

await test(`${TABLE} is what the script makes from the RAWmakase commit it names`, async () => {
  const tsv = readFileSync(join(root, TABLE), 'utf8');
  const source = recordedSource(tsv);
  assert.ok(source, 'the table has no "# Source:" line');
  let baselines;
  try {
    baselines = await fetchBaselines(source.sha);
  } catch (error) {
    if (inCI) throw error;
    return `could not fetch RAWmakase's record: ${error.cause?.code ?? error.message}`;
  }
  const photos = goldenPhotos(readFileSync(join(root, SAMPLES), 'utf8'));
  const made = regenerate(tsv, baselines, photos, source);
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
