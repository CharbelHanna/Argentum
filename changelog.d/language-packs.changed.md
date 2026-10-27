- **Only the interface language in use is loaded.** All fifteen translations were built into the
  main script, so every start parsed about 1.1 MB of text for the one language shown. English
  stays built in; each other language is its own file of the build, read when it is the language
  in Settings (`src/argentum/locales/languages.ts`, Vite's `import.meta.glob`, the dynamic-import
  pattern i18next documents). The main script goes from 4.52 MB to 3.40 MB. Argentum's renames
  (Subject shown as Object, `renames.ts`) are applied to each language as it loads, and a first
  start still picks the system's language from every translation, not only the loaded ones.
