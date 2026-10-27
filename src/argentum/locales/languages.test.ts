// Languages load when picked (argentum/locales/languages.ts), and Argentum's
// renames (renames.ts) reach a language loaded after start, not only English.
import { describe, expect, it } from 'vitest';
import i18n from 'i18next';
import '../../i18n';
import { registerArgentumTranslations } from '.';
import { LANGUAGES, switchLanguage } from './languages';

registerArgentumTranslations();

describe('the interface languages', () => {
  it('are English and every locale file', () => {
    expect([...LANGUAGES].sort()).toEqual(
      ['ca', 'cs', 'de', 'en', 'es', 'fr', 'it', 'ja', 'ko', 'nl', 'pl', 'pt', 'ru', 'zh-CN', 'zh-TW'].sort(),
    );
  });

  it('start with English alone', () => {
    expect(i18n.hasResourceBundle('en', 'translation')).toBe(true);
    expect(i18n.hasResourceBundle('fr', 'translation')).toBe(false);
    expect(i18n.t('masks.types.subject')).toBe('Object');
  });

  it('load when picked, with the renames', async () => {
    await switchLanguage('fr');
    expect(i18n.language).toBe('fr');
    expect(i18n.t('masks.types.subject')).toBe('Objet');
    expect(i18n.t('settings.processing.ai.cpu.feature1')).not.toMatch(/sujet/i);
  });

  it('fall back to English for a language with no file, as before', async () => {
    await switchLanguage('xx');
    expect(i18n.t('masks.types.subject')).toBe('Object');
    await switchLanguage('en');
    expect(i18n.t('masks.types.subject')).toBe('Object');
  });
});
