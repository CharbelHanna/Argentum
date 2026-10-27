- **No call to Google at start.** `index.html` linked Poppins from Google Fonts, so every launch
  asked fonts.googleapis.com for a stylesheet, and offline it failed. Nothing drew in Poppins: the
  Modern look sets the interface font to Segoe UI or the system font (`modern.css`), and a walk of
  the whole interface with every Poppins face declared requested none. The links are gone, so the
  window looks exactly as before. The UI walk no longer strips them (`scripts/ui-walk/server.mjs`).
