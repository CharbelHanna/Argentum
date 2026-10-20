// Our lines in their files may only go down.
//
// WHY A RATCHET AND NOT A BUDGET
//
// The budgets this replaces were a number per file that a feature could spend
// up to, and they were spent: by 2026-10-18 our lines sat in 66 of their files,
// and every one of them is a place an upstream release can collide with us.
// Argentum is moving its code out of their files and into its own (the engine,
// mods/, src/argentum/). The ratchet holds that direction: for every file of
// theirs outside the render path and today's interface, the number of lines we
// have added to it is recorded in scripts/upstream-ratchet.json, and the build
// fails when a file's number goes above it.
//
// Lowering is the only edit. A local `npm run check:merge` that finds a file
// below its number, with nothing else wrong, writes the lower number back; CI
// fails until that file is committed, so slack is never left to spend later.
// The baseline itself is compared with the one on main where the branch began:
// raising a number by hand is a rise like any other.
//
// "Our lines" is what upstream-diff.mjs counts as added and ours: a line of
// theirs replaced in place counts once, and a borrowed fix of theirs not at all.

export const BASELINE = 'scripts/upstream-ratchet.json';

/**
 * Raises decided by name: the ratchet's one way up.
 *
 * A number may go above main's only when an entry here names the file, the
 * number it goes to, the day, and why the line has to sit in their file. It is
 * for a line their own code cannot run safely without, never for a feature
 * that could live in a file of ours. Entries stay as the record; once main
 * carries the number, the entry no longer changes anything.
 */
export const RAISES = [
  {
    file: 'src-tauri/src/exif_processing.rs',
    to: 4,
    on: '2026-10-20',
    why: 'RapidRAW 1.6.6 deletes a sidecar that holds only defaults (write_sidecar, is_default_sidecar). '
      + 'Argentum keeps Snapshots in the same sidecar (agSnapshots), and every sidecar writer is in their '
      + 'files, so the check that keeps a sidecar holding snapshots has to be the one line in theirs. '
      + 'Without it, resetting a photo that has snapshots deletes them.',
  },
];

const ABOUT =
  'Our lines in each RapidRAW file outside the render path and today\'s interface, measured against the '
  + 'upstream release we are merged up to. A number may only go down: npm run check:merge '
  + 'fails when a file rises above it, and writes the lower number here when it falls.';

/** The baseline file's text for a set of counts: sorted, one file per line. */
export function formatBaseline(counts) {
  const files = Object.fromEntries(
    [...counts.entries()].filter(([, n]) => n > 0).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );
  return `${JSON.stringify({ about: ABOUT, files }, null, 2)}\n`;
}

/** The counts in a baseline file's text, or null when there is none to read. */
export function parseBaseline(text) {
  if (text === null || text === undefined) return null;
  const parsed = JSON.parse(text);
  return new Map(Object.entries(parsed.files ?? {}));
}

/**
 * Compare the tree with the baseline, and the baseline with the one before it.
 *
 *   current   Map file -> our lines now
 *   baseline  Map file -> the committed numbers
 *   previous  Map file -> the numbers on main where this branch began, or null
 *   raises    the decided raises (RAISES): a number on one of them is not raised
 *
 * Returns:
 *   rises    files above their number: the build fails
 *   lowered  files below it: the number comes down
 *   raised   numbers higher than they were on main and not decided: the build fails
 */
export function compareRatchet(current, baseline, previous, raises = RAISES) {
  const rises = [];
  const lowered = [];
  const raised = [];
  for (const [file, now] of current) {
    const allowed = baseline.get(file) ?? 0;
    if (now > allowed) rises.push({ file, now, allowed });
  }
  for (const [file, allowed] of baseline) {
    const now = current.get(file) ?? 0;
    if (now < allowed) lowered.push({ file, from: allowed, to: now });
  }
  if (previous) {
    for (const [file, n] of baseline) {
      const was = previous.get(file) ?? 0;
      const decided = raises.some((r) => r.file === file && r.to === n);
      if (n > was && !decided) raised.push({ file, from: was, to: n });
    }
  }
  return { rises, lowered, raised };
}
