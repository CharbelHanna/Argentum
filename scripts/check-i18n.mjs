// The interface's translations (src/i18n/locales), checked against what main already had.
// Two checks, as `i18n:check` ran them while it could not fail a build:
//
//   * missing: a key the code uses, or a plural form a language needs, that a locale file lacks
//     (i18next-cli's extractor, configured in i18next.config.mjs, run without writing);
//   * runtime: what src/i18n/check-runtime.mjs finds when i18next resolves every plural form of
//     every language and compares each translation's placeholders with the English.
//
// The inherited files already fail both, so each issue is held in scripts/i18n-baseline.json:
//
//   - an issue the baseline does not list fails: a translation got worse;
//   - an issue the baseline lists that is gone also fails, until the baseline is rewritten, so a
//     fix is kept.
//
//   node scripts/check-i18n.mjs            check
//   node scripts/check-i18n.mjs --update   rewrite the baseline from the tree

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { basename, dirname, join } from 'node:path';

import { extract } from 'i18next-cli';

import config from '../i18next.config.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const baselinePath = join(root, 'scripts', 'i18n-baseline.json');

/** Dotted paths of every string in a nested translation object. */
function leaves(object, prefix = '', out = new Set()) {
  for (const [key, value] of Object.entries(object ?? {})) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object' && !Array.isArray(value)) leaves(value, path, out);
    else out.add(path);
  }
  return out;
}

/** `locale:key` for each key the extractor would add to a locale file. */
async function missingKeys() {
  const missing = [];
  for (const result of await extract(config)) {
    const locale = basename(result.path, '.json');
    const existing = leaves(result.existingTranslations);
    for (const key of leaves(result.newTranslations)) {
      if (!existing.has(key)) missing.push(`${locale}:${key}`);
    }
  }
  return missing.sort();
}

/** The issues check-runtime.mjs lists, one per line. */
function runtimeIssues() {
  const result = spawnSync(process.execPath, [join(root, 'src', 'i18n', 'check-runtime.mjs')], {
    cwd: root,
    encoding: 'utf8',
  });
  if (result.error) throw result.error;
  const issues = result.stderr
    .split('\n')
    .filter((line) => line.startsWith('- '))
    .map((line) => line.slice(2).trim());
  if (result.status !== 0 && issues.length === 0) {
    throw new Error(`check-runtime.mjs failed without listing an issue:\n${result.stderr}`);
  }
  return [...new Set(issues)].sort();
}

const now = { missing: await missingKeys(), runtime: runtimeIssues() };

if (process.argv.includes('--update')) {
  writeFileSync(baselinePath, `${JSON.stringify(now, null, 2)}\n`);
  console.log(`Wrote scripts/i18n-baseline.json: ${now.missing.length} missing, ${now.runtime.length} runtime.`);
} else {
  const before = JSON.parse(readFileSync(baselinePath, 'utf8'));
  const worse = [];
  const better = [];
  for (const kind of ['missing', 'runtime']) {
    const was = new Set(before[kind]);
    const is = new Set(now[kind]);
    worse.push(...now[kind].filter((issue) => !was.has(issue)).map((issue) => `${kind}: ${issue}`));
    better.push(...before[kind].filter((issue) => !is.has(issue)).map((issue) => `${kind}: ${issue}`));
  }

  if (worse.length > 0) {
    console.error(`Translation check: ${worse.length} new issue(s) against the baseline:\n`);
    for (const line of worse) console.error(`  ${line}`);
    console.error('\nAdd the key to every locale in src/i18n/locales, with its plural forms.');
    process.exitCode = 1;
  } else if (better.length > 0) {
    console.error(`Translation check: ${better.length} issue(s) fewer than the baseline:\n`);
    for (const line of better) console.error(`  ${line}`);
    console.error('\nKeep the gain: node scripts/check-i18n.mjs --update, and commit the baseline.');
    process.exitCode = 1;
  } else {
    console.log(
      `Translation check: matches the baseline (${now.missing.length} missing, ${now.runtime.length} runtime).`,
    );
  }
}
