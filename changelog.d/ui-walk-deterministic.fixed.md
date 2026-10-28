- **The UI walk (`npm run check:ui`) gives the same verdict on a busy machine as on an idle one.**
  A step now waits for the work the app puts off a moment (the History written 600 ms after a
  change, a photo's EXIF read 500 ms after it shows) before the next begins, so each backend call
  is listed under the shortcut that made it rather than one or more steps later. The app's
  messages are recorded and closed where they show; the walk looked for a class the app's toasts
  no longer carry and never saw one. A slow start fails the walk in CI only, where the machine runs
  nothing else; elsewhere it is printed with the limit CI holds it to.
