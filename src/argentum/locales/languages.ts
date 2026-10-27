/**
 * The languages the interface speaks, loaded when picked. Ours.
 *
 * English is built in (src/i18n/index.ts). The other fourteen locale files
 * were imported there too, so every start parsed all fifteen, 1.4 MB of text
 * for the one language shown. Now each is its own chunk of the build, read
 * only when it is the language in Settings: `import.meta.glob` is Vite's
 * dynamic import of a folder, the pattern i18next documents for bundled apps
 * (i18next-resources-to-backend, without the package).
 *
 * Switch with `switchLanguage`, never `i18n.changeLanguage` alone: i18next
 * would switch to a language it has no words for and show English, and the
 * renames (renames.ts) would never reach that language's words.
 */

import i18n from 'i18next';

import { applyRenames } from './renames';

const LOCALES = import.meta.glob<{ default: Record<string, unknown> }>([
  '../../i18n/locales/*.json',
  '!../../i18n/locales/en.json',
]);

/** Every language the interface has a file for, English included. */
export const LANGUAGES: readonly string[] = [
  'en',
  ...Object.keys(LOCALES).map((path) => path.replace(/^.*\/(.+)\.json$/, '$1')),
];

/** Languages being read or read, so two quick picks of one read it once. */
const loading = new Map<string, Promise<void>>();

/** Read a language's words into i18next, once; English is already there. */
function loadLanguage(lng: string): Promise<void> {
  const load = LOCALES[`../../i18n/locales/${lng}.json`];
  if (!load || i18n.hasResourceBundle(lng, 'translation')) return Promise.resolve();
  let loaded = loading.get(lng);
  if (!loaded) {
    loaded = load().then(({ default: words }) => {
      i18n.addResourceBundle(lng, 'translation', words);
      applyRenames();
    });
    loaded.catch(() => loading.delete(lng));
    loading.set(lng, loaded);
  }
  return loaded;
}

/** Load `lng` if it is not loaded yet, then show the interface in it. */
export async function switchLanguage(lng: string): Promise<void> {
  try {
    await loadLanguage(lng);
  } catch (error) {
    // A chunk that will not load leaves the interface in the language it has.
    console.error(`Could not load the ${lng} translation:`, error);
    return;
  }
  await i18n.changeLanguage(lng);
}
