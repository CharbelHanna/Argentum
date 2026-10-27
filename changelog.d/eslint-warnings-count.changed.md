- **Unused variables and untranslated text in the interface now fail the frontend check.** Two
  ESLint rules, `@typescript-eslint/no-unused-vars` and `i18next/no-literal-string`, were set to
  warn, and `scripts/check-frontend.mjs` counts errors only, so 35 unused locals and 11 literal
  strings got through unseen. Both are errors now, and `scripts/frontend-baseline.json` holds
  today's count per file: a new one fails, a removed one asks for `--update`.
