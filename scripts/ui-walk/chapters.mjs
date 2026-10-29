// What the walk visits, in order. Each chapter starts from a fresh load, so
// what one chapter clicked never changes what the next finds, and after any
// step that leaves the window changed (a photo opened, a toast, a panel gone)
// the chapter loads again before its next step. Inside a chapter the walk goes
// by what is on screen (every tab of a tab strip, every item of a menu), so a
// new tab or menu item is walked without a line here.
//
// Paths read the way a person gets there: "Library > Thumbnail menu > Rename".

import { comboName } from './session.mjs';

/** The library is ready when the demo folder's six photos show in the grid. */
export const LIBRARY_READY = { selector: '[data-bench-id="thumbnail"] img', count: 6, what: 'the demo folder' };
/** The editor is ready when the open photo's first frame is drawn. */
export const EDITOR_READY = { selector: '[data-bench-id="editor-first-frame"]', what: 'the demo photo in the editor' };

const TAB_STRIP = '.agm-right .agm-tabs [role="tab"]';
const TAB_BODY = '.agm-right .agm-tab-body';
/** Words on the buttons that close a dialog without doing anything. */
const CLOSE_WORDS = /^(cancel|close|no|not now|done|ok|icon x)$/i;
/** Menu items never pressed: they leave the app (the window's own close is a stub call, but a reload loops). */
const NEVER_PRESS = /^(quit|exit|restart|reload)\b/i;

function failWith(message) {
  throw new Error(message.slice(0, 2000));
}

const byLabel = (controls, label, kind) =>
  controls.find((c) => c.label === label && (!kind || c.kind === kind)) ??
  failWith(`no ${kind ?? 'control'} "${label}" on screen; there are: ${controls.map((c) => c.entry).join(', ')}`);

async function picked(w, selector, index = 0) {
  return (await w.pick(selector, index)) ?? failWith(`nothing on screen matches ${selector}`);
}

/**
 * A chapter's page: how to load it, and how to get it back after a step that changed it.
 * `setup` runs after every load (selecting a photo, say); `same` is how the window looks then.
 */
function place(w, chapter) {
  let same = null;
  // A message on screen is not a change of page: it is recorded and closed where it shows.
  const page = async () => JSON.stringify({ ...(await w.facts()), messages: '' });
  const start = async () => {
    await w.load(chapter.query, chapter.ready);
    await chapter.setup?.(w);
    same = await page();
  };
  const restore = async () => {
    if ((await page()) !== same) await start();
  };
  const changed = async () => (await page()) !== same;
  return { start, restore, changed, facts: () => JSON.parse(same) };
}

/** Close the dialog on top: Esc, else its Cancel or Close, else the walk says it cannot. */
async function closeDialog(w, path) {
  await w.press(['Escape'], `${path} (closing)`);
  if ((await w.layer()).kind !== 'dialog') return;
  const close = (await w.look()).find((c) => CLOSE_WORDS.test(c.label));
  if (close) await w.click(close, `${path} (closing)`);
}

/** After a press: record a dialog it opened (and close it), close a menu it left open, record its messages. */
async function afterPress(w, path) {
  const top = await w.layer();
  if (top.kind === 'dialog') {
    await w.record(path);
    await walkDialogTabs(w, path);
    await closeDialog(w, path);
  } else if (top.kind === 'menu') {
    await w.closeMenus();
  }
  await w.recordToasts(path);
}

/** A dialog with tabs of its own: each tab recorded. */
async function walkDialogTabs(w, path) {
  const tabs = (await w.look('[role="tab"]')).filter((tab) => !tab.disabled).map((tab) => tab.label);
  for (const label of tabs.slice(1)) {
    await w.click(byLabel(await w.look('[role="tab"]'), label, 'tab'), `${path} > ${label}`);
    await w.record(`${path} > ${label}`);
  }
}

/** Open the menu again and hover down `trail`, the labels of the submenus above the one walked. */
async function reopen(w, open, trail) {
  await open();
  for (const label of trail) {
    const before = await w.menuCount();
    const item = byLabel(await w.look({ deepest: true }), label, 'menuitem');
    await w.hover(item);
    if (!(await w.waitForMenus(before + 1))) {
      // An interactive submenu (tags, say) opens on a click rather than on hover.
      await w.click(item);
      await w.waitForMenus(before + 1);
    }
    await w.settle();
  }
}

/**
 * A menu and everything under it: record its items, go into each submenu, press each other item
 * and record the dialog it opens. `open` brings the menu up from the chapter's page.
 */
async function walkMenu(w, home, path, open, trail = []) {
  await home.restore();
  await reopen(w, open, trail);
  if (trail.length > 0 && (await w.menuCount()) <= trail.length) return; // nothing opened under it
  const items = await w.record(path, { deepest: true });
  const labels = items.filter((item) => item.kind === 'menuitem').map((item) => item.label);
  let open_ = true;
  for (const label of labels) {
    const itemPath = `${path} > ${label}`;
    if (!open_) {
      await home.restore();
      await reopen(w, open, trail);
    }
    const item = (await w.look({ deepest: true })).find((c) => c.label === label && c.kind === 'menuitem');
    open_ = true;
    if (!item || item.disabled || NEVER_PRESS.test(label)) continue;
    if (item.submenu) {
      await walkMenu(w, home, itemPath, open, [...trail, label]);
      open_ = false;
      continue;
    }
    await w.click(item, itemPath);
    await afterPress(w, itemPath);
    open_ = false;
  }
  await w.closeMenus();
}

/** Every tab of the right panel, and the Info line, each recorded, with any tab strip inside it. */
async function walkRightPanel(w, path) {
  const tabs = (await w.look(TAB_STRIP)).map((tab) => tab.label);
  const visit = async (control, tabPath) => {
    // What a tab calls when it opens (the Export tab's size estimate) shows only there.
    const callsBefore = await w.evaluate('({ ...window.__uiWalk.calls })');
    await w.click(control, tabPath);
    await w.record(tabPath, { scope: TAB_BODY });
    const called = calledBetween(callsBefore, await w.evaluate('({ ...window.__uiWalk.calls })'));
    if (called.length > 0) w.note(`${tabPath} > calls ${called.join(', ')}`);
    const inner = (await w.look(`${TAB_BODY} [role="tab"]`)).map((tab) => tab.label);
    for (const label of inner) {
      await w.click(byLabel(await w.look(TAB_BODY), label, 'tab'), `${tabPath} > ${label}`);
      await w.record(`${tabPath} > ${label}`, { scope: TAB_BODY });
    }
  };
  for (const label of tabs) await visit(byLabel(await w.look(TAB_STRIP), label, 'tab'), `${path} > ${label}`);
  const info = (await w.look('.agm-right .agm-info-head'))[0];
  if (info) await visit(info, `${path} > Info`);
}

/** Every drop-down of the top bar, opened, recorded and closed. Choosing in them only sorts or shows. */
async function walkTopMenus(w, path) {
  const menus = (await w.look('.agm-top .agm-menu-wrap > button')).map((menu) => menu.label);
  for (const label of menus) {
    const name = label.replace(/:.*$/, '');
    await w.click(byLabel(await w.look('.agm-top'), label), `${path} > ${name}`);
    await w.record(`${path} > ${name}`);
    await w.closeMenus();
  }
}

/** A shortcut's key as Settings > Controls writes it ("Ctrl + Shift + C", "←"): pressing it records a new key. */
const KEY_BUTTON =
  /^((Ctrl|Shift|Alt|Meta|⌘|⌥) \+ )*(.{1,2}|Enter|Delete|Space|Backspace|Tab|Esc|Escape|F\d{1,2}|PrtSc)$/;

/**
 * Every button of a Settings section pressed once, and the dialog it opens recorded and closed. Most
 * choose something and the page stays; the walk loads again only when one changed the page.
 */
async function walkSettingsButtons(w, home, section) {
  const path = `Settings > ${section}`;
  const openSection = async () => {
    await w.click((await w.look('.agm-top .agm-gear'))[0], path);
    await w.click(byLabel(await w.look('.agm-settings-nav'), section), path);
    return w.facts();
  };
  await home.restore();
  let page = JSON.stringify(await openSection());
  const buttons = (await w.look('.agm-settings-body button')).filter((b) => b.kind === 'button' && !b.disabled);
  for (const label of [...new Set(buttons.map((b) => b.label))]) {
    if (NEVER_PRESS.test(label) || KEY_BUTTON.test(label)) continue;
    const button = (await w.look('.agm-settings-body button')).find((b) => b.label === label);
    if (!button) continue;
    await w.click(button, `${path} > ${label}`);
    await afterPress(w, `${path} > ${label}`);
    if (
      JSON.stringify({ ...(await w.facts()), messages: '' }) !== JSON.stringify({ ...JSON.parse(page), messages: '' })
    ) {
      await home.restore();
      page = JSON.stringify(await openSection());
    }
  }
}

/** What changed between two facts(), in words; the messages that appeared are part of it. */
export function outcome(before, after) {
  const show = (value) => (value === '' || value === 0 ? 'none' : String(value));
  const changes = Object.keys(after)
    .filter((key) => JSON.stringify(after[key]) !== JSON.stringify(before[key]))
    .map((key) => `${key} ${show(before[key])} -> ${show(after[key])}`);
  return changes.length > 0 ? changes.join('; ') : 'no visible change';
}

/**
 * Calls that come when they please rather than from the key: the window's own, the log, the
 * auto sharpening asked in the background, settings saved after a pause, and the AI connector's
 * status, asked on a timer.
 */
const BACKGROUND_CALLS =
  /^(plugin:(window|event)\|.*|frontend_log|ag:sharpen_auto|save_settings|check_ai_connector_status)$/;

/** The app's commands called between two counts of the stub's calls, the background's left out. */
export function calledBetween(before, after) {
  return Object.keys(after)
    .filter((name) => after[name] > (before[name] ?? 0) && !BACKGROUND_CALLS.test(name))
    .sort();
}

/**
 * Every default shortcut pressed from the chapter's page, and what it did there: what changed on
 * screen, and which of the app's commands it called (a rating or a copy shows only there).
 */
async function walkShortcuts(w, home, context) {
  const shortcuts = await w.evaluate(
    `import('/src/utils/keyboardUtils.ts').then((m) => m.KEYBIND_DEFINITIONS.map((d) => ({ action: d.action, combo: m.getDefaultCombo(d, 'windows') })))`,
  );
  for (const { action, combo } of shortcuts) {
    await home.restore();
    const path = `Shortcuts > ${context} > ${comboName(combo)} (${action})`;
    const before = await w.facts();
    const callsBefore = await w.evaluate('({ ...window.__uiWalk.calls })');
    await w.press(combo, path);
    w.note(`${path} > ${outcome(before, await w.facts())}`);
    const called = calledBetween(callsBefore, await w.evaluate('({ ...window.__uiWalk.calls })'));
    if (called.length > 0) w.note(`${path} > calls ${called.join(', ')}`);
    const top = await w.layer();
    if (top.kind === 'dialog') await closeDialog(w, path);
    else if (top.kind === 'menu') await w.closeMenus();
    await w.recordToasts(path);
  }
}

/** Select the first photo in the grid, so that what works on the selection has one. */
async function selectFirstPhoto(w) {
  await w.click(await picked(w, '[data-bench-id="thumbnail"]'), 'selecting the first photo');
}

export const CHAPTERS = [
  {
    name: 'Library',
    query: '',
    ready: LIBRARY_READY,
    async walk(w) {
      await w.record('Library', { skip: TAB_BODY });
      await walkTopMenus(w, 'Library > Top bar');
      await walkRightPanel(w, 'Library > Right panel');
    },
  },
  {
    name: 'Library menus',
    query: '',
    ready: LIBRARY_READY,
    setup: selectFirstPhoto,
    async walk(w, home) {
      await walkMenu(w, home, 'Library > Thumbnail menu', async () =>
        w.rightClick(await picked(w, '[data-bench-id="thumbnail"]'), 'Library > Thumbnail menu'),
      );
      await walkMenu(w, home, 'Library > Grid menu', async () =>
        w.rightClick(await picked(w, '.agm-grid-view'), 'Library > Grid menu', 'bottom'),
      );
      await walkMenu(w, home, 'Library > Folder menu', async () => {
        const folders = (await w.look('.agm-folders')).filter((c) => c.label.startsWith('Demo'));
        await w.rightClick(folders.at(-1) ?? failWith('no Demo folder in the folder column'), 'Library > Folder menu');
      });
    },
  },
  {
    name: 'Settings',
    query: '',
    ready: LIBRARY_READY,
    async walk(w, home) {
      const gear = (await w.look('.agm-top .agm-gear'))[0] ?? failWith('no Settings button in the top bar');
      await w.click(gear, 'Settings');
      await w.record('Settings', { scope: '.agm-settings-nav' });
      const sections = (await w.look('.agm-settings-nav .agm-settings-item')).map((item) => item.label);
      for (const label of sections) {
        await w.click(byLabel(await w.look('.agm-settings-nav'), label), `Settings > ${label}`);
        await w.record(`Settings > ${label}`, { scope: '.agm-settings-body' });
      }
      for (const label of sections) await walkSettingsButtons(w, home, label);
    },
  },
  {
    name: 'Editor',
    query: '?open',
    ready: EDITOR_READY,
    async walk(w) {
      await w.record('Editor', { skip: TAB_BODY });
      await walkTopMenus(w, 'Editor > Top bar');
      await walkRightPanel(w, 'Editor > Right panel');
    },
  },
  {
    name: 'Editor menus',
    query: '?open',
    ready: EDITOR_READY,
    async walk(w, home) {
      await walkMenu(w, home, 'Editor > Photo menu', async () =>
        w.rightClick(await picked(w, '.agm-view'), 'Editor > Photo menu'),
      );
    },
  },
  {
    name: 'Shortcuts in the library',
    query: '',
    ready: LIBRARY_READY,
    setup: selectFirstPhoto,
    walk: (w, home) => walkShortcuts(w, home, 'Library'),
  },
  {
    name: 'Shortcuts in the editor',
    query: '?open',
    ready: EDITOR_READY,
    walk: (w, home) => walkShortcuts(w, home, 'Editor'),
  },
];

/** Run one chapter from its first load. */
export async function walkChapter(w, chapter) {
  const home = place(w, chapter);
  await home.start();
  await chapter.walk(w, home);
}
