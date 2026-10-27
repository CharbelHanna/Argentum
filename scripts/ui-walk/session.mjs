// One walk through the page: load it, look (page.mjs), click, press keys, and
// keep what was reached and every error the page reported along the way.
// What to walk is in chapters.mjs; this is how.

import { installWalker } from './page.mjs';

/** CDP's modifier bits. */
const MODIFIERS = { alt: 1, ctrl: 2, meta: 4, shift: 8 };

/** key, keyCode and the character typed, for the codes the shortcuts use. */
const NAMED_KEYS = {
  Enter: ['Enter', 13, '\r'],
  Escape: ['Escape', 27],
  Delete: ['Delete', 46],
  Backspace: ['Backspace', 8],
  Tab: ['Tab', 9],
  Space: [' ', 32, ' '],
  ArrowLeft: ['ArrowLeft', 37],
  ArrowUp: ['ArrowUp', 38],
  ArrowRight: ['ArrowRight', 39],
  ArrowDown: ['ArrowDown', 40],
  Equal: ['=', 187, '='],
  Minus: ['-', 189, '-'],
  Comma: [',', 188, ','],
  Period: ['.', 190, '.'],
  Slash: ['/', 191, '/'],
  BracketLeft: ['[', 219, '['],
  BracketRight: [']', 221, ']'],
};
const SHIFTED_DIGITS = ')!@#$%^&*(';

/** The key event for a shortcut code (`KeyA`, `Digit1`, `Enter`) on a US layout. */
export function keyOf(code, shift) {
  if (/^Key[A-Z]$/.test(code)) {
    const letter = shift ? code.slice(3) : code.slice(3).toLowerCase();
    return { key: letter, keyCode: code.charCodeAt(3), text: letter };
  }
  if (/^Digit\d$/.test(code)) {
    const digit = Number(code.slice(5));
    const key = shift ? SHIFTED_DIGITS[digit] : String(digit);
    return { key, keyCode: 48 + digit, text: key };
  }
  if (/^F\d{1,2}$/.test(code)) return { key: code, keyCode: 111 + Number(code.slice(1)) };
  const named = NAMED_KEYS[code];
  if (!named) throw new Error(`The walk cannot press ${code}: add it to NAMED_KEYS in session.mjs.`);
  return { key: named[0], keyCode: named[1], text: named[2] };
}

/** "ctrl+shift+KeyC" as a person reads it: "Ctrl+Shift+C". */
export const comboName = (combo) =>
  combo
    .map(
      (part) => ({ ctrl: 'Ctrl', shift: 'Shift', alt: 'Alt', meta: 'Meta' })[part] ?? part.replace(/^(Key|Digit)/, ''),
    )
    .join('+');

/** A console message's words, with its %s and %d filled in, as DevTools shows it. */
export function consoleText(args) {
  const words = args.map((arg) => (arg.type === 'string' ? arg.value : (arg.description ?? String(arg.value))));
  if (typeof words[0] === 'string' && /%[sdoOif]/.test(words[0])) {
    let rest = words.slice(1);
    const head = words[0].replace(/%[sdoOif]/g, () => (rest.length ? String(rest.shift()) : ''));
    words.splice(0, words.length, head, ...rest);
  }
  return words.join(' ');
}

export async function openSession({ page, url, keys, verbose = false }) {
  const reached = new Set();
  const errors = [];
  const rawKeys = new Map();
  /** Steps whose page never went quiet within settle's timeout. */
  const unsettled = [];
  let step = 'loading';
  let domReady = null;
  page.on('Page.domContentEventFired', () => domReady?.());

  page.on('Runtime.consoleAPICalled', ({ type, args }) => {
    if (type === 'error' || type === 'assert') errors.push({ text: consoleText(args), at: step });
  });
  page.on('Runtime.exceptionThrown', ({ exceptionDetails: d }) => {
    errors.push({ text: `Uncaught ${d.exception?.description ?? d.text}`, at: step });
  });
  page.on('Log.entryAdded', ({ entry }) => {
    if (entry.level === 'error') errors.push({ text: `${entry.text} ${entry.url ?? ''}`.trim(), at: step });
  });
  // alert(), confirm() and prompt() would stop the page until answered: each is recorded, then dismissed.
  page.on('Page.javascriptDialogOpening', ({ type, message }) => {
    reached.add(`${step} > browser ${type} "${message}"`);
    page.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {});
  });
  await page.send('Runtime.enable');
  await page.send('Log.enable');
  await page.send('Page.enable');
  await page.send('Page.addScriptToEvaluateOnNewDocument', {
    source: `(${installWalker.toString()})(${JSON.stringify(keys)});`,
  });

  /** The value of an expression in the page, awaited. */
  async function evaluate(expression) {
    const { result, exceptionDetails } = await page.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (exceptionDetails)
      throw new Error(`In the page: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`);
    return result.value;
  }
  const call = (name, ...args) => evaluate(`window.__walker.${name}(${args.map((a) => JSON.stringify(a)).join(', ')})`);

  async function settle() {
    const start = Date.now();
    const quiet = await call('settle');
    if (!quiet) unsettled.push(step);
    if (verbose) console.log(`  ${String(Date.now() - start).padStart(5)} ms${quiet ? '' : ' (never quiet)'}  ${step}`);
  }

  /** Load the app at `query` and wait for `ready`; resolves with the milliseconds from navigation to ready. */
  async function load(query, ready) {
    step = `loading ${query || '/'}`;
    let timer;
    const parsed = new Promise((resolve, reject) => {
      domReady = resolve;
      timer = setTimeout(() => reject(new Error(`The page at ${query || '/'} did not load within a minute.`)), 60000);
    });
    await page.send('Page.navigate', { url: `${url}${query}` });
    await parsed.finally(() => clearTimeout(timer));
    const ms = await call('waitFor', ready.selector, ready.count ?? 1);
    if (ms === null) {
      const shows = await evaluate(`document.getElementById('root')?.innerText.replace(/\\s+/g, ' ').slice(0, 400)`);
      const said = errors.map((e) => e.text.split('\n')[0].slice(0, 200)).slice(-5);
      throw new Error(
        `The app did not show ${ready.what} within a minute of loading ${query || '/'}. ` +
          `It shows: "${shows ?? ''}". Errors so far: ${said.length ? said.join(' | ') : 'none'}.`,
      );
    }
    await settle();
    return Math.round(ms);
  }

  /** Record what can be reached now under `path`: page.mjs controls(), with its `scope` and `skip`. */
  async function record(path, where = {}) {
    const start = Date.now();
    const { controls } = await call('controls', where);
    for (const control of controls) reached.add(`${path} > ${control.entry}`);
    const listed = Date.now();
    for (const key of await call('rawKeys')) if (!rawKeys.has(key)) rawKeys.set(key, path);
    if (verbose)
      console.log(`  ${String(listed - start).padStart(5)} ms list, ${Date.now() - listed} ms keys  ${path}`);
    return controls;
  }

  /** The controls of the top layer now, without recording them: page.mjs controls(), `scope` a selector or its options. */
  const look = async (scope) =>
    (await call('controls', typeof scope === 'object' && scope ? scope : { scope: scope ?? null })).controls;
  const layer = async () => (await call('controls', {})).layer;
  /** The element `selector` finds, to click like a control; null when there is none. */
  const pick = (selector, index = 0) => call('pick', selector, index);

  async function mouse(x, y, button, clicks) {
    await page.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
    for (let count = 1; count <= clicks; count += 1) {
      const buttons = button === 'left' ? 1 : button === 'right' ? 2 : 0;
      await page.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button, buttons, clickCount: count });
      await page.send('Input.dispatchMouseEvent', {
        type: 'mouseReleased',
        x,
        y,
        button,
        buttons: 0,
        clickCount: count,
      });
    }
  }

  /**
   * Point at a control (from look, record or pick) and press, with the pointer as a person does, or with
   * a click from inside the page when something covers it. `spot` 'bottom' presses below what a list holds.
   */
  async function act(control, { button = 'left', clicks = 1, at, spot } = {}) {
    step = at ?? step;
    const where = await call('point', control.id, spot ?? 'centre');
    if (!where) throw new Error(`${step}: "${control.label}" is not on screen to click.`);
    if (where.covered && button === 'left' && clicks === 1) await call('press', control.id);
    else if (button === 'none')
      await page.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: where.x, y: where.y });
    else await mouse(where.x, where.y, button, clicks);
    await settle();
  }

  const click = (control, at) => act(control, { at });
  const doubleClick = (control, at) => act(control, { clicks: 2, at });
  const rightClick = (control, at, spot) => act(control, { button: 'right', at, spot });
  const hover = (control, at) => act(control, { button: 'none', at });

  /** Press a shortcut (["ctrl", "KeyZ"]) with nothing focused, as from the window. */
  async function press(combo, at) {
    step = at ?? step;
    await evaluate(
      'document.activeElement && document.activeElement !== document.body && document.activeElement.blur()',
    );
    const modifiers = combo.slice(0, -1).reduce((bits, part) => bits | (MODIFIERS[part] ?? 0), 0);
    const code = combo.at(-1);
    const { key, keyCode, text } = keyOf(code, (modifiers & MODIFIERS.shift) !== 0);
    const typed = text && (modifiers & (MODIFIERS.ctrl | MODIFIERS.alt | MODIFIERS.meta)) === 0;
    const event = { modifiers, key, code, windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode };
    await page.send('Input.dispatchKeyEvent', {
      ...event,
      type: typed ? 'keyDown' : 'rawKeyDown',
      ...(typed ? { text } : {}),
    });
    await page.send('Input.dispatchKeyEvent', { ...event, type: 'keyUp' });
    // A key the app leaves alone is the browser's: Ctrl+1 goes to its first tab, and a page in the
    // background draws nothing. The app's window has no tabs, so the page comes back to the front.
    await page.send('Page.bringToFront');
    await settle();
  }

  /** Close open menus with a press outside them. */
  async function closeMenus() {
    await call('pressOutside');
    await settle();
  }

  /** Record the messages on screen under `path`, then close them. */
  async function recordToasts(path) {
    const texts = await call('toasts');
    for (const text of texts) reached.add(`${path} > message "${text}"`);
    if (texts.length > 0) {
      await call('dismissToasts');
      await settle();
    }
  }

  return {
    reached,
    errors,
    rawKeys,
    unsettled,
    evaluate,
    load,
    record,
    look,
    layer,
    pick,
    menuCount: () => call('menuCount'),
    waitForMenus: (count) => call('waitForMenus', count),
    click,
    doubleClick,
    rightClick,
    hover,
    press,
    closeMenus,
    recordToasts,
    settle,
    facts: () => call('facts'),
    /** Record an entry the page cannot list: what a shortcut did. */
    note: (entry) => reached.add(entry),
    at: (where) => (step = where),
    calls: () =>
      evaluate(
        'window.__uiWalk ? { unanswered: Object.keys(window.__uiWalk.unanswered), offline: Object.keys(window.__uiWalk.offline) } : null',
      ),
  };
}
