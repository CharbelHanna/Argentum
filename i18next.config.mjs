import { defineConfig } from 'i18next-cli';

export default defineConfig({
  locales: ['en', 'de', 'pl', 'zh-CN', 'zh-TW', 'es', 'fr', 'it', 'pt', 'ja', 'ko', 'ru', 'ca', 'nl', 'cs'],
  extract: {
    input: ['src/**/*.{ts,tsx}'],
    output: 'src/i18n/locales/{{language}}.json',
    defaultNS: false,
    removeUnusedKeys: false,
    sort: true,
    defaultValue: '',
    // Argentum's own strings live in src/argentum/locales (namespace `ag`), and TypeScript already
    // checks their keys against its en.json (AgKey). Left alone, this extractor reads every
    // `t = useAgTranslation()` call as one of these files' keys and would copy ours into them.
    ignoreNamespaces: ['ag'],
  },
  plugins: [
    {
      name: 'argentum-namespace',
      // A file that translates with useAgTranslation only has none of these keys (its `t` may also
      // arrive as a prop). In one that uses both, each `x = useAgTranslation()` reads as
      // useTranslation('ag') would.
      onLoad: (code) =>
        code.includes('useAgTranslation') && !code.includes("from 'react-i18next'")
          ? '// Argentum strings only'
          : code.replace(/const (\w+) = useAgTranslation\(\);/g, "const { t: $1 } = useTranslation('ag');"),
    },
  ],
});
