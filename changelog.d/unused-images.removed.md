- **2.4 MB of images nothing showed.** The three theme splash photos (`public/splash-*.jpg`, 2.36
  MB in every installer) and the `splashImage` field nothing read since the start screen went;
  `public/tauri.svg`; `public/vite.svg`, the template favicon (the icon link is now an empty
  `data:` URL, so the window asks for no file); `src/assets/react.svg`; and in `src-tauri/icons`,
  the Android and iOS icon sets (the Android project keeps its own in `gen/android`) and three
  unused preview PNGs. 44 files, 3.37 MB of the repository; the built frontend goes from 7.31 MB
  to 4.95 MB.
