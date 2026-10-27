import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import en from './locales/en.json';

// Argentum: English only here; the other languages load when picked (argentum/locales/languages.ts).
i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
  },
  lng: 'en',
  fallbackLng: 'en',
  returnEmptyString: false,
  interpolation: {
    escapeValue: false,
  },
});
