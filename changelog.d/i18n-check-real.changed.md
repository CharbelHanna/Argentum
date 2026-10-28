- **The translation check can fail.** CI's `i18n:check` was advisory (`continue-on-error`), and
  on CI it never got as far as checking: i18next-cli needs Node 22 and the lint job ran Node 20.
  Run by hand, it failed on main for two reasons. Its extractor read every
  `t = useAgTranslation()` call as a key missing from these files (279 of them, Argentum's own
  strings, which TypeScript already checks against `src/argentum/locales/en.json`); it now leaves
  the `ag` namespace out. What remains is real: 330 missing keys or plural forms (Catalan, French,
  Spanish, Italian and Portuguese lack `_many`, Czech, Polish and Russian `_few` and `_many` on a
  few keys, every language but English `adjustments.basic.toneGroup`) and 207 plural resolutions
  that fall back to English. They are held in `scripts/i18n-baseline.json` by
  `scripts/check-i18n.mjs`: a new gap fails, and so does a fix until the baseline is rewritten
  (`--update`), the same rule as `scripts/frontend-baseline.json`. The lint job moves to Node 22,
  the release builds' version. `i18next.config.ts` becomes `.mjs` so the check can import it on
  any Node.
