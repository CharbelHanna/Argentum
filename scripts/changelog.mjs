// Changelog entries, one file per change, folded into CHANGELOG.md at release.
//
// WHY THIS EXISTS
//
// Every pull request added its entry at the same spot at the top of
// CHANGELOG.md, so each merge put every other open PR in conflict, and
// updating them re-ran all of CI. Now a PR adds its own file and nothing
// else touches CHANGELOG.md until a release.
//
// The shape is towncrier's (https://towncrier.readthedocs.io): a folder of
// fragments named `<topic>.<section>.md`, the section in the name. Here the
// folder is `changelog.d/`, a fragment holds its bullet(s) as they will read
// in CHANGELOG.md, and the sections are CHANGELOG.md's own.
//
//   node scripts/changelog.mjs           fold changelog.d/ into the top
//                                        version of CHANGELOG.md, delete the
//                                        fragments (a release step)
//   node scripts/changelog.mjs --check   fail on a fragment with no known section

import { readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const SECTIONS = ['Added', 'Changed', 'Fixed', 'Removed'];
export const FOLDER = 'changelog.d';

/** A fragment's section from its file name (`atomic-write.fixed.md` -> `Fixed`), or null. */
export function sectionOf(name) {
  const match = /^[^.]+\.([a-z]+)\.md$/.exec(name);
  return SECTIONS.find((s) => s.toLowerCase() === match?.[1]) ?? null;
}

/**
 * `changelog` with each fragment's text added at the end of its section in the
 * top `## ` version, the section made when that version has none yet (in
 * SECTIONS order). `fragments` is `[{ name, text }]`, folded in name order.
 */
export function fold(changelog, fragments) {
  const lines = changelog.split('\n');
  const top = lines.findIndex((l) => l.startsWith('## '));
  if (top < 0) throw new Error('CHANGELOG.md has no "## <version>" heading to fold into');
  let end = lines.findIndex((l, i) => i > top && l.startsWith('## '));
  if (end < 0) end = lines.length;

  for (const { name, text } of [...fragments].sort((a, b) => a.name.localeCompare(b.name))) {
    const section = sectionOf(name);
    if (!section) throw new Error(`${FOLDER}/${name}: name it <topic>.<${SECTIONS.join('|').toLowerCase()}>.md`);
    const entry = text.replace(/\s+$/, '').split('\n');

    let heading = lines.findIndex((l, i) => i > top && i < end && l === `### ${section}`);
    if (heading < 0) {
      // Before the first later section, or at the end of the version.
      const later = SECTIONS.slice(SECTIONS.indexOf(section) + 1).map((s) => `### ${s}`);
      let at = lines.findIndex((l, i) => i > top && i < end && later.includes(l));
      if (at < 0) at = end;
      lines.splice(at, 0, `### ${section}`, '', '');
      heading = at;
      end += 3;
    }
    // The section's last non-blank line, before the next heading.
    let last = heading;
    for (let i = heading + 1; i < end && !lines[i].startsWith('#'); i++) {
      if (lines[i].trim()) last = i;
    }
    if (last === heading) last += 1; // the blank line under a new heading
    lines.splice(last + 1, 0, ...entry);
    end += entry.length;
  }
  return lines.join('\n');
}

/** The fragments in `root`'s changelog.d, as `[{ name, text }]`. */
export function readFragments(root) {
  let names;
  try {
    names = readdirSync(join(root, FOLDER));
  } catch {
    return [];
  }
  return names
    .filter((n) => n.endsWith('.md') && n !== 'README.md')
    .map((name) => ({ name, text: readFileSync(join(root, FOLDER, name), 'utf8') }));
}

function main() {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const fragments = readFragments(root);
  const bad = fragments.filter((f) => !sectionOf(f.name)).map((f) => f.name);
  if (bad.length) {
    console.error(`${FOLDER}: no known section in ${bad.join(', ')}; name it <topic>.<section>.md`);
    process.exitCode = 1;
    return;
  }
  if (process.argv.includes('--check')) {
    console.log(`${FOLDER}: ${fragments.length} fragment(s), all named right`);
    return;
  }
  const path = join(root, 'CHANGELOG.md');
  writeFileSync(path, fold(readFileSync(path, 'utf8'), fragments));
  for (const { name } of fragments) rmSync(join(root, FOLDER, name));
  console.log(`CHANGELOG.md: folded ${fragments.length} fragment(s)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
