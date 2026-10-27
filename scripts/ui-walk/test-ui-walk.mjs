// Tests for the UI walk's own logic (check-ui.mjs, session.mjs, chapters.mjs,
// browser.mjs): what counts as an allowed error, an entry gone, a slow start, a
// key pressed. The walk itself needs a browser: npm run check:ui.
//
// Run:  npm run test:checks   (or node scripts/ui-walk/test-ui-walk.mjs)

import assert from 'node:assert/strict';

import { findBrowser } from './browser.mjs';
import { outcome } from './chapters.mjs';
import { compareReachable, slowStart, sortErrors } from './check-ui.mjs';
import { comboName, consoleText, keyOf } from './session.mjs';

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

console.log('ui-walk');

test('an error the list allows is not unexpected, and an entry nothing matched is unused', () => {
  const allowed = [
    { match: 'ResizeObserver loop', reason: 'a browser warning' },
    { match: 'never seen', reason: 'gone since' },
  ];
  const errors = [
    { text: 'ResizeObserver loop completed with undelivered notifications', at: 'Library' },
    { text: 'TypeError: x is undefined', at: 'Editor' },
  ];
  const { unexpected, unused } = sortErrors(errors, allowed);
  assert.deepEqual(unexpected, [errors[1]]);
  assert.deepEqual(
    unused.map((entry) => entry.match),
    ['never seen'],
  );
});

test('an entry that went and one that appeared are both reported', () => {
  const { appeared, gone } = compareReachable(['A > b [button]', 'A > c [tab]'], ['A > c [tab]', 'A > d [menuitem]']);
  assert.deepEqual(gone, ['A > b [button]']);
  assert.deepEqual(appeared, ['A > d [menuitem]']);
});

test('a start fails only past three times the recorded time and at least 3 s over', () => {
  assert.equal(slowStart('Library', 9000, 3000), null);
  assert.match(slowStart('Library', 9001, 3000), /over 9000 ms/);
  // A fast recorded start gets the 3 s of slack, not three times almost nothing.
  assert.equal(slowStart('Editor', 3500, 500), null);
  assert.match(slowStart('Editor', 3501, 500), /over 3500 ms/);
  assert.equal(slowStart('Editor', 99999, undefined), null);
});

test('a shortcut code becomes the key a US keyboard sends', () => {
  assert.deepEqual(keyOf('KeyA', false), { key: 'a', keyCode: 65, text: 'a' });
  assert.deepEqual(keyOf('KeyA', true), { key: 'A', keyCode: 65, text: 'A' });
  assert.deepEqual(keyOf('Digit3', true), { key: '#', keyCode: 51, text: '#' });
  assert.deepEqual(keyOf('F5', false), { key: 'F5', keyCode: 116 });
  assert.equal(keyOf('ArrowLeft', false).keyCode, 37);
  assert.throws(() => keyOf('NumpadMultiply', false), /NAMED_KEYS/);
  assert.equal(comboName(['ctrl', 'shift', 'KeyC']), 'Ctrl+Shift+C');
});

test("a console message's %s are filled in as DevTools shows it", () => {
  const args = [
    { type: 'string', value: 'Failed %s: %d' },
    { type: 'string', value: 'load' },
    { type: 'number', value: 3, description: '3' },
  ];
  assert.equal(consoleText(args), 'Failed load: 3');
  assert.equal(consoleText([{ type: 'object', description: 'Error: boom' }]), 'Error: boom');
});

test('what a shortcut did is told in words, or no visible change', () => {
  assert.equal(outcome({ photo: 'a.jpg', dialog: '' }, { photo: 'b.jpg', dialog: '' }), 'photo a.jpg -> b.jpg');
  assert.equal(outcome({ dialog: '' }, { dialog: 'Rename' }), 'dialog none -> Rename');
  assert.equal(outcome({ photo: 'a.jpg' }, { photo: 'a.jpg' }), 'no visible change');
});

test('the browser named in the environment wins over the installed ones', () => {
  assert.equal(findBrowser({ UI_WALK_BROWSER: '/opt/edge' }, 'linux'), '/opt/edge');
  assert.equal(findBrowser({ CHROME_PATH: '/opt/chrome' }, 'linux'), '/opt/chrome');
  assert.throws(() => findBrowser({}, 'plan9'), /UI_WALK_BROWSER/);
});

console.log(`\n${ran - failed}/${ran} passed`);
if (failed > 0) process.exitCode = 1;
