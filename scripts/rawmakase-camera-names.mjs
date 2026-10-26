// RAWmakase's names for the cameras Adobe's profiles call otherwise, regenerated.
//
// A camera profile (an Adobe DCP) names the body it was made for, and the
// profile list and the render keep a profile only for that body
// (src-tauri/src/mods/profiles.rs `matches`). Adobe profiles some bodies under
// another body's name, or spells a make its own way: the Lumix G97 and its
// twins under the G99, the E-M10 Mark III S under the Mark III, Samsung's
// SM-G965F as the Galaxy S9+. RAWmakase keeps those names in
// data/adobe-camera-names.toml, each checked against what Camera Raw installs.
// This script writes Argentum's copy from one RAWmakase release:
//
//   * one source version at a time, named on the table's "# Source:" line;
//   * the rows are generated, never edited by hand;
//   * check:sources warns when RAWmakase releases past the version named.
//
// Run:  node scripts/rawmakase-camera-names.mjs <release tag, or full commit>
// Test: node scripts/test-rawmakase-camera-names.mjs
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { releases } from './rawmakase-cameras.mjs';

export const SOURCE_PATH = 'data/adobe-camera-names.toml';
export const TABLE = 'src-tauri/src/mods/adobe_camera_names.tsv';

/**
 * The [[make]] and [[camera]] rows of data/adobe-camera-names.toml, each field as
 * written there.
 *
 * Not a TOML parser: the file is flat `key = "string"` lines under those headers,
 * and anything else stops here, so a change of format is read by a person rather
 * than misread.
 */
export function parseNames(toml) {
  const rows = [];
  let row = null;
  toml.split('\n').forEach((raw, i) => {
    const line = raw.trim();
    const where = `adobe-camera-names.toml line ${i + 1}`;
    if (line === '' || line.startsWith('#')) return;
    if (line === '[[make]]' || line === '[[camera]]') {
      row = { kind: line.slice(2, -2) };
      rows.push(row);
      return;
    }
    const pair = /^([A-Za-z0-9_]+)\s*=\s*"([^"\\\t]*)"\s*$/.exec(line);
    if (!pair || !row) throw new Error(`${where}: not a key = "string" line under [[make]] or [[camera]]: ${line}`);
    row[pair[1]] = pair[2];
  });
  for (const r of rows) {
    const needs = r.kind === 'make' ? ['libraw', 'adobe'] : ['make', 'model', 'adobe'];
    const missing = needs.filter((k) => !r[k]);
    if (missing.length > 0) throw new Error(`a [[${r.kind}]] row has no ${missing.join(', ')}: ${JSON.stringify(r)}`);
  }
  return rows;
}

/** One line per name: kind, make, model ("-" for a make), Adobe's name, why. */
export function tableRows(rows) {
  return rows.map((r) =>
    r.kind === 'make'
      ? ['make', r.libraw, '-', r.adobe, r.why ?? ''].join('\t')
      : ['camera', r.make, r.model, r.adobe, r.why ?? ''].join('\t'),
  );
}

/** The "# Source:" line: which RAWmakase commit, its release, and what the rows hold. */
export function sourceLine({ sha, tag }, rows) {
  const count = (kind) => rows.filter((r) => r.startsWith(`${kind}\t`)).length;
  return (
    `# Source: RAWmakase ${SOURCE_PATH} @ ${sha}${tag ? ` (${tag})` : ''}, ` +
    `${count('make')} make, ${count('camera')} cameras`
  );
}

/** The commit and release a table names, or null if it names none. */
export function recordedSource(tsv) {
  const m = /^# Source: RAWmakase \S+ @ ([0-9a-f]{40})(?: \((v[^)]+)\))?,/m.exec(tsv);
  return m ? { sha: m[1], tag: m[2] ?? null } : null;
}

/** The table rewritten from the source: our comments kept, the Source line and rows replaced. */
export function regenerate(tsv, toml, source) {
  const lines = tsv.split('\n');
  const end = lines.findIndex((l) => !l.startsWith('#'));
  const header = lines.slice(0, end === -1 ? lines.length : end);
  const at = header.findIndex((l) => l.startsWith('# Source:'));
  if (at === -1) throw new Error(`${TABLE} has no "# Source:" line in its header`);
  const rows = tableRows(parseNames(toml));
  header[at] = sourceLine(source, rows);
  return `${[...header, ...rows].join('\n')}\n`;
}

/** The source at one commit. */
export async function fetchNames(sha) {
  const url = `https://raw.githubusercontent.com/pch/rawmakase/${sha}/${SOURCE_PATH}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(20000) });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.text();
}

/** For check:sources: a warning when RAWmakase has released past the table, else null. */
export async function namesWarning(root) {
  // A checkout without the table (the e2e test's throwaway forks) has nothing to check.
  if (!existsSync(join(root, TABLE))) return null;
  const recorded = recordedSource(readFileSync(join(root, TABLE), 'utf8'));
  if (!recorded) return `${TABLE} names no RAWmakase source - node scripts/rawmakase-camera-names.mjs <release>`;
  let tags;
  try {
    tags = await releases();
  } catch (error) {
    return `could not list RAWmakase's releases, so ${TABLE} is unchecked (${error.code ?? error.message})`;
  }
  const [newest, sha] = [...tags].at(-1) ?? [];
  if (!newest || sha === recorded.sha) return null;
  const at = recorded.tag ?? `commit ${recorded.sha.slice(0, 8)}, not a release`;
  return `RAWmakase ${newest} is out and the camera names are at ${at} - node scripts/rawmakase-camera-names.mjs ${newest}`;
}

async function main(ref) {
  if (!ref) throw new Error('usage: node scripts/rawmakase-camera-names.mjs <release tag, or full commit>');
  const tags = await releases();
  let source;
  if (tags.has(ref)) source = { sha: tags.get(ref), tag: ref };
  else if (/^[0-9a-f]{40}$/.test(ref)) source = { sha: ref, tag: [...tags].find(([, s]) => s === ref)?.[0] ?? null };
  else throw new Error(`${ref} is not a RAWmakase release tag or a full 40-character commit`);

  const root = join(dirname(fileURLToPath(import.meta.url)), '..');
  const path = join(root, TABLE);
  const before = readFileSync(path, 'utf8');
  const after = regenerate(before, await fetchNames(source.sha), source);
  writeFileSync(path, after);
  console.log(`  ${TABLE}: ${source.tag ?? 'commit'} ${source.sha.slice(0, 8)}`);
  console.log(before === after ? '  unchanged' : '  changed - git diff it before merging');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main(process.argv[2]).catch((error) => {
    console.error(`  ${error.message}`);
    process.exitCode = 1;
  });
}
