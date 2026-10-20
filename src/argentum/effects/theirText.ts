/**
 * RapidRAW's Relight and Fog strings, by their keys. Ours.
 *
 * Their keys carry their translations into every language they ship, so the
 * Relight and Fog parts use them, and argentum/locales/renames.ts puts our
 * English wording over the ones said differently (Lamp, Existing light...).
 *
 * The typed `t` only knows the keys in their en.json, and this branch's copy
 * predates 1.6.6's Relight and Fog strings. Until it has them, a key is passed
 * through untyped here, and renames.ts supplies the English.
 */

import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import type { ParseKeys } from 'i18next';

export function useTheirText() {
  const { t } = useTranslation();
  return useCallback(
    (key: string, options?: Record<string, unknown>): string => t(key as ParseKeys, options ?? {}) as string,
    [t],
  );
}
