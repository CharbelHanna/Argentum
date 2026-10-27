// Headless Edge or Chrome, driven over the DevTools protocol on a pipe. No
// WebSocket and no port: Chrome reads commands from its fd 3 and answers on its
// fd 4, one JSON message per NUL, the way Playwright and Puppeteer talk to it.
//
// The browser is the one already on the machine: UI_WALK_BROWSER (or
// CHROME_PATH) when set, else the usual install places of Edge and Chrome.

import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const INSTALLED = {
  win32: [
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  ],
  linux: [
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/usr/bin/microsoft-edge',
  ],
  darwin: [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
  ],
};

/** The browser to drive: the one named in the environment, else the first installed. */
export function findBrowser(env = process.env, platform = process.platform) {
  const named = env.UI_WALK_BROWSER || env.CHROME_PATH;
  if (named) return named;
  const found = (INSTALLED[platform] ?? []).find((path) => existsSync(path));
  if (!found) throw new Error('No Edge or Chrome found: set UI_WALK_BROWSER to its executable.');
  return found;
}

/** One DevTools connection: commands with answers, and events by name. */
class Connection {
  #next = 0;
  #pending = new Map();
  #handlers = new Map();
  #buffered = [];
  #gone = null;

  constructor(toBrowser, fromBrowser) {
    this.toBrowser = toBrowser;
    fromBrowser.on('data', (chunk) => this.#receive(chunk));
    toBrowser.on('error', (error) => this.abandon(error.message));
  }

  #receive(chunk) {
    let start = 0;
    let end = chunk.indexOf(0);
    while (end !== -1) {
      this.#buffered.push(chunk.subarray(start, end));
      this.#dispatch(JSON.parse(Buffer.concat(this.#buffered).toString('utf8')));
      this.#buffered = [];
      start = end + 1;
      end = chunk.indexOf(0, start);
    }
    if (start < chunk.length) this.#buffered.push(chunk.subarray(start));
  }

  #dispatch(message) {
    if (message.id !== undefined) {
      const waiting = this.#pending.get(message.id);
      this.#pending.delete(message.id);
      if (message.error) waiting?.reject(new Error(`${waiting.method}: ${message.error.message}`));
      else waiting?.resolve(message.result);
      return;
    }
    for (const handler of this.#handlers.get(message.method) ?? []) handler(message.params, message.sessionId);
  }

  send(method, params = {}, sessionId = undefined) {
    if (this.#gone) return Promise.reject(new Error(`${method}: ${this.#gone}`));
    const id = ++this.#next;
    this.toBrowser.write(`${JSON.stringify({ id, method, params, sessionId })}\0`);
    return new Promise((resolve, reject) => this.#pending.set(id, { method, resolve, reject }));
  }

  on(method, handler) {
    this.#handlers.set(method, [...(this.#handlers.get(method) ?? []), handler]);
  }

  /** Fail every command still waiting, when the browser has gone. */
  abandon(reason) {
    this.#gone ??= reason;
    for (const { method, reject } of this.#pending.values()) reject(new Error(`${method}: ${reason}`));
    this.#pending.clear();
  }
}

/**
 * Start the browser with an empty profile and open one page of the given size.
 * Returns { page, close }: page.send(method, params) and page.on(event, handler)
 * talk to that page.
 */
export async function launchBrowser({ executable, width, height }) {
  const profile = mkdtempSync(join(tmpdir(), 'argentum-ui-walk-'));
  const args = [
    '--headless=new',
    '--remote-debugging-pipe',
    `--user-data-dir=${profile}`,
    `--window-size=${width},${height}`,
    '--force-device-scale-factor=1',
    '--hide-scrollbars',
    '--lang=en-US',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--disable-sync',
    '--disable-default-apps',
    '--disable-component-update',
    '--disable-background-networking',
    '--disable-background-timer-throttling',
    '--disable-backgrounding-occluded-windows',
    '--disable-renderer-backgrounding',
    '--disable-features=Translate,MediaRouter,OptimizationHints',
    '--mute-audio',
    // No network but the page's own server, whatever asks (fetch, images, sockets, workers).
    '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1',
    // GitHub's Ubuntu runners allow no user namespaces, so Chrome's sandbox cannot start there.
    ...(process.platform === 'linux' ? ['--no-sandbox', '--disable-dev-shm-usage'] : []),
    'about:blank',
  ];
  // Some Windows shells set an application-compatibility layer that makes Edge exit at once, doing nothing.
  const env = { ...process.env, __COMPAT_LAYER: undefined };
  const child = spawn(executable, args, { env, stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk) => (stderr = (stderr + chunk).slice(-4000)));
  const connection = new Connection(child.stdio[3], child.stdio[4]);
  const exited = new Promise((resolve) => child.once('exit', resolve));
  child.once('exit', (code) => connection.abandon(`the browser exited (${code}): ${stderr.trim().slice(-600)}`));
  child.once('error', (error) => connection.abandon(error.message));

  let page;
  try {
    // The tab the browser opened with, the only one, as the app's window has no tabs.
    const { targetInfos } = await connection.send('Target.getTargets');
    const { targetId } = targetInfos.find((target) => target.type === 'page');
    const { sessionId } = await connection.send('Target.attachToTarget', { targetId, flatten: true });
    page = {
      send: (method, params) => connection.send(method, params, sessionId),
      on: (event, handler) => connection.on(event, (params, from) => from === sessionId && handler(params)),
    };
    // The page's own size, whatever the window around it: the same layout on every machine.
    await page.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
    await page.send('Emulation.setTimezoneOverride', { timezoneId: 'UTC' });
  } catch (error) {
    child.kill();
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
    throw error;
  }

  const close = async () => {
    const asked = connection.send('Browser.close').catch(() => {});
    // Edge can take most of a minute to put its profile away; a throwaway profile needs none of that.
    const timer = setTimeout(() => child.kill(), 2000);
    await Promise.race([asked.then(() => exited), exited]);
    clearTimeout(timer);
    try {
      rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
    } catch {
      // A file the browser's last process still holds: the folder is in the system's temp, which clears it.
    }
  };
  return { page, close };
}
