// The interface, walked. A deletion that leaves a screen behind it broken, or a
// control nobody can reach any more, compiles and passes every test; this
// opens today's frontend in headless Edge or Chrome over a stub of the Tauri
// bridge (stub/), goes through it (chapters.mjs) and checks three things:
//
//   * what a person can reach (every view, tab, Settings section, menu, the
//     dialogs they open, and what each default shortcut does) matches
//     scripts/ui-walk/ui-reachability.json, one entry per line. An entry that
//     appeared or went fails until the file is rewritten in the same change,
//     so the pull request's diff shows exactly what a removal took away;
//   * the page reported no error: no console.error, no uncaught error or
//     rejection, no failed load, except those in allowed-errors.json, each
//     with its reason. An allowed error that no longer happens fails too,
//     until it is taken off the list;
//   * no text on screen is a raw i18n key, which is what i18next shows for a
//     translation it cannot find.
//
// It also times the start: from navigation to the library showing the demo
// folder, and to the editor showing the demo photo. The times are printed and
// fail past three times those recorded (and at least 3 s over), so a slow
// machine never fails it but a start that got much slower does. They fail in
// CI only, where the machine runs nothing else: a start recorded at 1.4 s took
// 5.0 s on a laptop compiling in other windows, and a wall-clock time has no
// limit that is right for both. Elsewhere a slow start is printed, with the
// limit CI holds it to; CI=1 npm run check:ui fails on it there too.
//
//   npm run check:ui              check
//   npm run check:ui -- --update  rewrite ui-reachability.json from the walk

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { findBrowser, launchBrowser } from './browser.mjs';
import { CHAPTERS, EDITOR_READY, LIBRARY_READY, walkChapter } from './chapters.mjs';
import { startServer } from './server.mjs';
import { openSession } from './session.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const reachabilityPath = join(here, 'ui-reachability.json');
const allowedPath = join(here, 'allowed-errors.json');
const toRepoPath = (file) => relative(root, file).split(sep).join('/');
const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'));

/** The page's size: wide enough that no column folds itself away. */
export const VIEWPORT = { width: 1600, height: 1000 };
/** Start-up may take this many times the recorded time, and at least SLACK_MS more, before it fails. */
export const STARTUP_CEILING = 3;
export const SLACK_MS = 3000;

/** What tells a raw key from words: the locales' top-level names, and Argentum's own keys. */
export function translationKeys(theirs, ours) {
  return { top: Object.keys(theirs).sort(), ag: Object.keys(ours).sort() };
}

/** Errors split into those the list allows (and which entries allowed one) and the rest. */
export function sortErrors(errors, allowed) {
  const used = new Set();
  const unexpected = [];
  for (const error of errors) {
    const entry = allowed.find((a) => error.text.includes(a.match));
    if (entry) used.add(entry.match);
    else unexpected.push(error);
  }
  return { unexpected, unused: allowed.filter((a) => !used.has(a.match)) };
}

/** Entries in `now` and not `before`, and the other way round. */
export function compareReachable(before, now) {
  const was = new Set(before);
  const is = new Set(now);
  return { appeared: now.filter((e) => !was.has(e)), gone: before.filter((e) => !is.has(e)) };
}

/** Whether a slow start fails the walk: in CI, the machine is the walk's alone. */
export const startupGated = (env) => !!env.CI && env.CI !== 'false' && env.CI !== '0';

/** A start-up time over its ceiling, or null. */
export function slowStart(name, ms, recordedMs) {
  if (!recordedMs) return null;
  const ceiling = Math.max(recordedMs * STARTUP_CEILING, recordedMs + SLACK_MS);
  return ms > ceiling
    ? `${name} ready in ${ms} ms, over ${ceiling} ms (${STARTUP_CEILING}x the ${recordedMs} recorded)`
    : null;
}

async function walk() {
  const keys = translationKeys(
    readJson(join(root, 'src', 'i18n', 'locales', 'en.json')),
    readJson(join(root, 'src', 'argentum', 'locales', 'en.json')),
  );
  const verbose = process.argv.includes('--verbose');
  const lap = (() => {
    let last = Date.now();
    return (what) => {
      if (verbose) console.log(`  ${String(Date.now() - last).padStart(5)} ms  ${what}`);
      last = Date.now();
    };
  })();
  const server = await startServer(root);
  lap('Vite started');
  let browser = null;
  try {
    browser = await launchBrowser({ executable: findBrowser(), ...VIEWPORT });
    lap('browser started');
    const w = await openSession({ page: browser.page, url: server.url, keys, verbose });
    // The first loads transform the app's modules; the times are the fastest of the next two.
    await w.load('', LIBRARY_READY);
    await w.load('?open', EDITOR_READY);
    const fastest = async (query, ready) => Math.min(await w.load(query, ready), await w.load(query, ready));
    const startup = { libraryMs: await fastest('', LIBRARY_READY), editorMs: await fastest('?open', EDITOR_READY) };
    const unanswered = new Set();
    const offline = new Set();
    const stopped = [];
    for (const chapter of CHAPTERS) {
      try {
        await walkChapter(w, chapter);
      } catch (error) {
        // The next chapter starts from a fresh load, so one broken screen does not hide the others.
        stopped.push(`${chapter.name}: ${error.message}`);
      }
      const calls = await w.calls().catch(() => null);
      for (const name of calls?.unanswered ?? []) unanswered.add(name);
      for (const origin of calls?.offline ?? []) offline.add(origin);
    }
    return { w, startup, stopped, unanswered: [...unanswered].sort(), offline: [...offline].sort() };
  } finally {
    lap('walked');
    await browser?.close();
    lap('browser closed');
    await server.close();
    lap('Vite closed');
  }
}

async function writeReachability(startup, reachable) {
  const text = JSON.stringify({ startupMs: startup, reachable }, null, 2);
  const prettier = await import('prettier');
  const options = { ...(await prettier.resolveConfig(reachabilityPath)), parser: 'json' };
  writeFileSync(reachabilityPath, await prettier.format(text, options));
}

async function main() {
  const started = Date.now();
  const update = process.argv.includes('--update');
  const { w, startup, stopped, unanswered, offline } = await walk();
  const reachable = [...w.reached].sort();
  const seconds = ((Date.now() - started) / 1000).toFixed(0);
  const times = `library ready in ${startup.libraryMs} ms, editor in ${startup.editorMs} ms`;
  const failures = [];

  if (stopped.length > 0) {
    failures.push(`${stopped.length} chapter(s) could not be walked to the end:`);
    for (const line of stopped) failures.push(`  ${line.split('\n')[0]}`);
  }
  const allowed = readJson(allowedPath).errors;
  const { unexpected, unused } = sortErrors(w.errors, allowed);
  if (unexpected.length > 0) {
    failures.push(`${unexpected.length} error(s) in the page:`);
    for (const error of unexpected) failures.push(`  ${error.at}: ${error.text.split('\n')[0].slice(0, 300)}`);
    failures.push(`  Fix them, or add one to ${toRepoPath(allowedPath)} with the reason it is expected.`);
  }
  if (unused.length > 0) {
    failures.push(`${unused.length} allowed error(s) no longer happen; take them off ${toRepoPath(allowedPath)}:`);
    for (const entry of unused) failures.push(`  ${entry.match}`);
  }
  if (w.rawKeys.size > 0) {
    failures.push(`${w.rawKeys.size} raw i18n key(s) on screen, translations missing from the locale files:`);
    for (const [key, path] of w.rawKeys) failures.push(`  ${path}: ${key}`);
  }

  if (update) {
    await writeReachability(startup, reachable);
    console.log(`UI walk: wrote ${toRepoPath(reachabilityPath)}, ${reachable.length} entries; ${times}.`);
  } else {
    const before = readJson(reachabilityPath);
    const { appeared, gone } = compareReachable(before.reachable, reachable);
    if (appeared.length + gone.length > 0) {
      failures.push(`${appeared.length} entr(ies) appeared and ${gone.length} went, against the reachability file:`);
      for (const entry of gone) failures.push(`  - ${entry}`);
      for (const entry of appeared) failures.push(`  + ${entry}`);
      failures.push(
        '  If that is what the change meant, say so in the pull request, then: npm run check:ui -- --update',
      );
    }
    const gated = startupGated(process.env);
    for (const slow of [
      slowStart('Library', startup.libraryMs, before.startupMs?.libraryMs),
      slowStart('Editor', startup.editorMs, before.startupMs?.editorMs),
    ]) {
      if (slow && gated) failures.push(`Start-up got slower: ${slow}.`);
      else if (slow)
        console.log(`Start-up slower than CI allows, not failed outside CI (a busy machine is slow too): ${slow}.`);
    }
  }

  if (w.unsettled.length > 0) {
    console.log(`Steps still changing after 6 s, recorded as they were: ${[...new Set(w.unsettled)].join(', ')}.`);
  }
  if (unanswered.length > 0) console.log(`Calls the stub answered with null: ${unanswered.join(', ')}.`);
  if (offline.length > 0) console.log(`Fetches refused, as offline: ${offline.join(', ')}.`);
  if (failures.length > 0) {
    console.error(`UI walk failed (${seconds} s):\n`);
    for (const line of failures) console.error(line.startsWith('  ') ? line : `\n${line}`);
    process.exitCode = 1;
  } else if (!update) {
    console.log(
      `UI walk: ${reachable.length} reachable entries match, no errors, no raw keys; ${times} (${seconds} s).`,
    );
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().catch((error) => {
    console.error(`UI walk could not run: ${error.stack ?? error}`);
    process.exitCode = 1;
  });
}
