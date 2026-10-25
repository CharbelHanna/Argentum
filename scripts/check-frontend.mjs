// The frontend's types, lint and formatting, checked against what main already
// had. The inherited code carries type errors, lint errors and unformatted
// files that nobody blocked on, so a plain `tsc` or `eslint` in CI would fail
// every pull request; with nothing blocking, a deletion that breaks an import
// passed every check. This counts each tool's issues per file (tsc by error
// code, ESLint errors by rule, Prettier by file) and compares them with
// scripts/frontend-baseline.json:
//
//   - more of anything than the baseline fails: something got worse;
//   - fewer also fails, until the baseline is rewritten, so a fix is kept.
//
//   node scripts/check-frontend.mjs            check
//   node scripts/check-frontend.mjs --update   rewrite the baseline from the tree

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative, sep } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const baselinePath = join(root, 'scripts', 'frontend-baseline.json');
const toRepoPath = (file) => relative(root, file).split(sep).join('/');

function runTool(script, args) {
  const result = spawnSync(process.execPath, [join(root, 'node_modules', ...script), ...args], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  return result.stdout;
}

function countInto(counts, file, key) {
  counts[file] ??= {};
  counts[file][key] = (counts[file][key] ?? 0) + 1;
}

/** Type errors per file and error code. */
function typeErrors() {
  const counts = {};
  const out = runTool(['typescript', 'bin', 'tsc'], ['--noEmit', '--pretty', 'false']);
  for (const line of out.split('\n')) {
    const match = /^(.+?)\(\d+,\d+\): error (TS\d+):/.exec(line);
    if (match) countInto(counts, match[1].replaceAll('\\', '/'), match[2]);
  }
  return counts;
}

/** ESLint errors (not warnings) per file and rule. */
function lintErrors() {
  const counts = {};
  const out = runTool(['eslint', 'bin', 'eslint.js'], ['.', '--format', 'json']);
  for (const result of JSON.parse(out)) {
    for (const message of result.messages) {
      if (message.severity === 2) countInto(counts, toRepoPath(result.filePath), message.ruleId ?? 'parse');
    }
  }
  return counts;
}

/** Files Prettier would change. */
function unformattedFiles() {
  const out = runTool(['prettier', 'bin', 'prettier.cjs'], ['--list-different', '.']);
  return out
    .split('\n')
    .map((line) => line.trim().replaceAll('\\', '/'))
    .filter(Boolean)
    .sort();
}

function sortKeys(counts) {
  return Object.fromEntries(
    Object.keys(counts)
      .sort()
      .map((file) => [file, Object.fromEntries(Object.entries(counts[file]).sort())]),
  );
}

/** Lines saying where `now` has more or fewer than `before`, per file and key. */
function compareCounts(tool, before, now) {
  const worse = [];
  const better = [];
  for (const file of new Set([...Object.keys(before), ...Object.keys(now)])) {
    for (const key of new Set([...Object.keys(before[file] ?? {}), ...Object.keys(now[file] ?? {})])) {
      const was = before[file]?.[key] ?? 0;
      const is = now[file]?.[key] ?? 0;
      if (is > was) worse.push(`${tool}: ${file} ${key}: ${was} -> ${is}`);
      if (is < was) better.push(`${tool}: ${file} ${key}: ${was} -> ${is}`);
    }
  }
  return { worse, better };
}

const now = { tsc: sortKeys(typeErrors()), eslint: sortKeys(lintErrors()), prettier: unformattedFiles() };

if (process.argv.includes('--update')) {
  writeFileSync(baselinePath, `${JSON.stringify(now, null, 2)}\n`);
  console.log(`Wrote ${toRepoPath(baselinePath)}.`);
} else {
  const before = JSON.parse(readFileSync(baselinePath, 'utf8'));
  const types = compareCounts('tsc', before.tsc, now.tsc);
  const lint = compareCounts('eslint', before.eslint, now.eslint);
  const worse = [
    ...types.worse,
    ...lint.worse,
    ...now.prettier.filter((f) => !before.prettier.includes(f)).map((f) => `prettier: ${f} is not formatted`),
  ];
  const better = [
    ...types.better,
    ...lint.better,
    ...before.prettier.filter((f) => !now.prettier.includes(f)).map((f) => `prettier: ${f} is formatted now`),
  ];

  if (worse.length > 0) {
    console.error(`Frontend check: ${worse.length} new issue(s) against the baseline:\n`);
    for (const line of worse) console.error(`  ${line}`);
    console.error('\nFix them. Formatting: npx prettier --write <file>.');
    process.exitCode = 1;
  } else if (better.length > 0) {
    console.error(`Frontend check: ${better.length} issue(s) fewer than the baseline:\n`);
    for (const line of better) console.error(`  ${line}`);
    console.error('\nKeep the gain: node scripts/check-frontend.mjs --update, and commit the baseline.');
    process.exitCode = 1;
  } else {
    console.log('Frontend check: types, lint and formatting match the baseline.');
  }
}
