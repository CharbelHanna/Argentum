// The Tauri bridge for today's frontend in a plain browser. scripts/ui-walk/
// server.mjs loads this ahead of src/main.tsx, so the app finds a window, an
// OS and a backend before its first module runs.
//
// The bridge itself is Tauri's own test double (@tauri-apps/api/mocks: invoke,
// callbacks, events, the window's label); the answers are in answers.js. What
// this adds is what a walk needs: a fresh start on every load, animations that
// end at once, and a count of the calls still running so the walker knows when
// the app has settled.

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { MotionGlobalConfig } from 'framer-motion';
import i18n from 'i18next';
import { answer } from './answers.js';
import { photoAt } from './photos.js';

// Every load starts from the stub's settings, never from a layout an earlier walk left behind.
try {
  localStorage.clear();
  sessionStorage.clear();
} catch {
  // Storage is off in this browser: nothing to clear.
}

// framer-motion's switch for tests: every animation jumps to its end.
MotionGlobalConfig.skipAnimations = true;

const walk = { pending: 0, calls: {}, unanswered: {}, offline: {} };
window.__uiWalk = walk;

const emit = (event, payload) => window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event, payload });

/** Run `work` later, counted as pending until it has run: an event the "backend" sends after answering. */
const later = (work, ms = 0) => {
  walk.pending += 1;
  setTimeout(() => {
    walk.pending -= 1;
    work();
  }, ms);
};

mockWindows('main');
mockIPC(
  async (cmd, args) => {
    const name = cmd === 'ag' ? `ag:${args?.name}` : cmd;
    walk.calls[name] = (walk.calls[name] ?? 0) + 1;
    walk.pending += 1;
    try {
      return await answer(cmd, args ?? {}, { emit, later, unanswered: () => (walk.unanswered[name] = true) });
    } finally {
      walk.pending -= 1;
    }
  },
  { shouldMockEvents: true },
);

// The double forgets a listener only when told its `id`, and @tauri-apps/api/event sends `eventId`:
// without this, every listener React's strict mode drops stays, and an event finds its callback gone.
const invoke = window.__TAURI_INTERNALS__.invoke;
window.__TAURI_INTERNALS__.invoke = (cmd, args, options) =>
  invoke(cmd, cmd === 'plugin:event|unlisten' ? { ...args, id: args.eventId } : args, options);

// No network: a fetch that leaves the page's own server fails as it does offline.
const pageFetch = window.fetch.bind(window);
window.fetch = (input, init) => {
  const url = new URL(input instanceof Request ? input.url : String(input), window.location.href);
  if (url.origin === window.location.origin || url.protocol === 'data:' || url.protocol === 'blob:') {
    return pageFetch(input, init);
  }
  walk.offline[url.origin] = true;
  return Promise.reject(new TypeError('Failed to fetch'));
};

// A file's URL in the app (thumbnails, previews): the demo photo's picture, else an empty pixel.
const EMPTY_PNG =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
window.__TAURI_INTERNALS__.convertFileSrc = (path) => photoAt(String(path).replace(/\//g, '\\'))?.dataUrl ?? EMPTY_PNG;

// @tauri-apps/plugin-os reads these at once, without a call.
window.__TAURI_OS_PLUGIN_INTERNALS__ = {
  platform: 'windows',
  os_type: 'windows',
  family: 'windows',
  version: '10.0.26100',
  arch: 'x86_64',
  eol: '\r\n',
  exe_extension: 'exe',
};

// A translation the app asks for and the locale lacks is an error, on screen or not: the text a
// deleted locale section leaves behind (tethering.title) reads like a key to no one else.
i18n.on('initialized', () => {
  i18n.options.saveMissing = true;
  i18n.options.missingKeyHandler = (_languages, _namespace, key) => console.error(`i18n: missing key ${key}`);
});
